# Logseq Text Editor Architecture Analysis

**Date**: October 15, 2025
**Analyst**: Claude Code
**Purpose**: Investigate Logseq's text editing implementation to inform NodeSpace's markdown editor architecture

---

## Executive Summary

Logseq uses **textarea + AST rendering** approach, NOT contenteditable. This is fundamentally different from NodeSpace's current implementation.

### Key Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      User Interface                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────────┐        ┌──────────────────────┐  │
│  │   Textarea Editor    │        │   Rendered View      │  │
│  │  (Plain Markdown)    │        │   (Formatted AST)    │  │
│  │                      │        │                      │  │
│  │  - Auto-resizing     │◄──────►│  - Markdown parsed   │  │
│  │  - Single source     │  SWAP  │  - Styled rendering  │  │
│  │  - No formatting     │        │  - Read-only display │  │
│  └──────────────────────┘        └──────────────────────┘  │
│           │                                   ▲              │
│           │ onChange                          │              │
│           ▼                                   │              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         State Management (Rum/React)                 │   │
│  │  - Single content state                              │   │
│  │  - Block-level granularity                           │   │
│  │  - No DOM sync issues                                │   │
│  └──────────────────────────────────────────────────────┘   │
│           │                                                  │
│           ▼                                                  │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         Markdown Parser (mldoc)                      │   │
│  │  - Parses to AST                                     │   │
│  │  - ClojureScript wrapper                             │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

**Why It Works**:
1. **Single Source of Truth**: Textarea value is the only state
2. **No DOM Synchronization**: No innerHTML vs state conflicts
3. **Simple Cursor Management**: Native textarea selection API
4. **Clear Separation**: Edit (plain text) vs View (formatted)

---

## Core Components Breakdown

### 1. Editor Component (`ls-textarea`)

**Location**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/ui.cljs` (lines 104-145)

**Implementation**:
```clojure
(rum/defc ls-textarea
  < rum/reactive
  {:did-mount (fn [state]
                (let [^js el (rum/dom-node state)
                      *mouse-point (volatile! nil)]
                  (doto el
                    (.addEventListener "select"
                                       #(let [start (util/get-selection-start el)
                                              end (util/get-selection-end el)]
                                          ;; Handle text selection events
                                          ...))
                    (.addEventListener "mouseup" #(vreset! *mouse-point {:x (.-x %) :y (.-y %)}))))
                state)}
  [{:keys [on-change] :as props}]
  (let [skip-composition? (state/sub :editor/action)
        on-composition (fn [e]
                         (if skip-composition?
                           (on-change e)
                           (case e.type
                             "compositionend" (do
                                                (state/set-editor-in-composition! false)
                                                (on-change e))
                             (state/set-editor-in-composition! true))))
        props (assoc props
                     "data-testid" "block editor"
                     :on-change (fn [e] (when-not (state/editor-in-composition?)
                                          (on-change e)))
                     :on-composition-start on-composition
                     :on-composition-update on-composition
                     :on-composition-end on-composition)]
    (textarea props)))
```

**Key Features**:
- Wraps `react-textarea-autosize` library for auto-resizing
- Handles IME (composition events) for international text input
- Tracks selection for plugin integration
- Pure textarea - no contenteditable tricks

**Library Used**: `react-textarea-autosize`
```clojure
["react-textarea-autosize" :as TextareaAutosize]
(defonce textarea (r/adapt-class (gobj/get TextareaAutosize "default")))
```

### 2. Editor Container (`box`)

**Location**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/components/editor.cljs` (lines 792-847)

**Architecture**:
```clojure
(rum/defcs box < rum/reactive
  {:init (fn [state]
           (assoc state
                  ::id (str (random-uuid))
                  ::ref (atom nil)))
   :did-mount (fn [state]
                (state/set-editor-args! (:rum/args state))
                state)
   :will-unmount (fn [state]
                   (state/set-state! :editor/raw-mode-block nil)
                   state)}
  (mixins/event-mixin setup-key-listener!)
  lifecycle/lifecycle
  [state {:keys [format block parent-block]} id config]
  (let [content (state/sub-edit-content (:block/uuid block))
        heading-class (get-editor-style-class block content format)
        opts {:id                id
              :ref               #(reset! *ref %)
              :cacheMeasurements (editor-row-height-unchanged?)
              :default-value     (or content "")
              :minRows           (if (state/enable-grammarly?) 2 1)
              :on-click          (editor-handler/editor-on-click! id)
              :on-change         (editor-handler/editor-on-change! block id search-timeout)
              :on-paste          (paste-handler/editor-on-paste! id)
              :on-key-down       ...
              :auto-focus true
              :auto-capitalize "off"
              :class heading-class}]
    [:div.editor-inner.flex.flex-1
     (ui/ls-textarea opts)
     (mock-textarea content)      ;; For cursor positioning calculations
     (command-popups id format)   ;; Autocomplete popups
     (when format
       (image-uploader id format))]))
```

**Critical Components**:
1. **`ls-textarea`**: The actual editable textarea
2. **`mock-textarea`**: Hidden div for calculating cursor positions (grapheme-aware)
3. **`command-popups`**: Slash commands, page search, block search autocomplete
4. **State subscriptions**: Reactive updates via Rum framework

### 3. Rendering System (View Mode)

**Location**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/components/block.cljs`

**Rendering Flow**:
```
Raw Markdown Text
      ↓
Parse to AST (mldoc parser)
      ↓
Block AST stored in state
      ↓
Render AST to React/Hiccup components
      ↓
Display formatted view
```

**Key Function**: `markup-element-cp` (line 4208)
```clojure
(defn markup-element-cp
  [{:keys [html-export?] :as config} item]
  (try
    (match item
      ["Paragraph" l]
      (->elem :div.is-paragraph (map-inline config l))

      ["Heading" h]
      (block-container config h)

      ["List" l]
      (->elem (list-element l) (map #(list-item config %) l))

      ["Math" s]
      (latex/latex s true true)

      ;; ... more AST node types
      )))
```

**Pattern Matching**: Uses `core.match` to render different AST node types

### 4. Mock Textarea (Cursor Positioning)

**Location**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/components/editor.cljs` (lines 637-663)

**Purpose**: Calculate accurate cursor positions for Unicode/grapheme clusters

```clojure
(rum/defc mock-textarea <
  rum/static
  {:did-update
   (fn [state]
     (when-not @(:editor/on-paste? @state/state)
       (try (editor-handler/handle-last-input)
            (catch :default _e
              nil)))
     (state/set-state! :editor/on-paste? false)
     state)}
  [content]
  [:div#mock-text
   {:style {:width "100%"
            :height "100%"
            :position "absolute"
            :visibility "hidden"
            :top 0
            :left 0}}
   (let [content (str content "0")
         graphemes (util/split-graphemes content)
         graphemes-char-index (reductions #(+ %1 (count %2)) 0 graphemes)]
     (for [[idx c] (into (sorted-map) (zipmap graphemes-char-index graphemes))]
       (if (= c "\n")
         [:span {:id (str "mock-text_" idx)
                 :key idx} "0" [:br]]
         [:span {:id (str "mock-text_" idx)
                 :key idx} c])))])
```

**Why Needed**:
- Emoji and Unicode characters can be multiple bytes
- Browser cursor APIs work with UTF-16 code units
- Mock textarea splits content by graphemes for accurate positioning

---

## State Management Architecture

### Single Source of Truth Pattern

**State Flow**:
```
User types in textarea
      ↓
onChange event fires
      ↓
State updates (via Rum atom)
      ↓
Component re-renders
      ↓
Textarea shows updated value
```

**Critical Code** (from `editor.cljs`):
```clojure
:on-change (editor-handler/editor-on-change! block id search-timeout)

;; In handler:
(defn editor-on-change! [block id search-timeout]
  (fn [e]
    (let [value (.-value (.-target e))
          block-uuid (:block/uuid block)]
      ;; Update state
      (state/set-edit-content! id value)
      ;; Trigger side effects (search, autocomplete, etc.)
      (handle-search-timeout! search-timeout value)
      ...)))
```

### Block-Level Granularity

- Each block (bullet point/paragraph) has its own editor instance
- Only one block can be in edit mode at a time
- Others display in rendered view mode
- **Switching**: Click on rendered block → enters edit mode (shows textarea)

### No DOM Synchronization Issues

**Why It Works**:
1. Textarea value is controlled by React state
2. No manual DOM manipulation
3. No need to preserve formatting in DOM
4. Cursor position handled by native textarea APIs

---

## Cursor Positioning Strategy

### Native Textarea Selection API

```javascript
// Get cursor position
input.selectionStart
input.selectionEnd

// Set cursor position
input.setSelectionRange(start, end)
```

**Logseq's Cursor Utilities** (`frontend.util.cursor`):
- `pos`: Get current cursor position
- `move-cursor-to`: Set cursor to specific position
- `get-caret-pos`: Get cursor coordinates for popup positioning
- `get-selection-start/end`: Get selection boundaries

### Grapheme-Aware Positioning

**Challenge**: Emoji and complex Unicode characters
**Solution**: Mock textarea with grapheme splitting

```clojure
;; Split content into graphemes (visual characters)
(util/split-graphemes content)

;; Each grapheme gets a span with ID
[:span {:id (str "mock-text_" idx)} c]

;; Calculate position by measuring spans
```

---

## Markdown Handling

### Parsing Strategy

**Parser**: `mldoc` (OCaml-based markdown parser compiled to JS)
- **Not shown in this codebase** (external library)
- Parses markdown to AST
- Supports Org-mode and Markdown formats

**AST Structure Examples**:
```clojure
["Paragraph" [...inline-elements...]]
["Heading" {...heading-data...}]
["List" [...list-items...]]
["Math" "latex-string"]
["Code" "language" "code-string"]
```

### Syntax Preservation

**In Edit Mode**: Plain text with markdown syntax visible
```
**bold text**
- [ ] todo item
[[page-link]]
```

**In View Mode**: Rendered with formatting
```
(bold text)
☐ todo item
→ page-link
```

**No Hybrid Rendering**: Never mix editable formatted text
- Contenteditable is avoided entirely
- All formatting happens in view mode
- Edit mode is always plain text

---

## Event Handling

### Key Events

**From `editor.cljs` setup**:
```clojure
(mixins/on-key-down
 state
 {}
 {:not-matched-handler (editor-handler/keydown-not-matched-handler format)})

(mixins/on-key-up
 state
 {}
 (editor-handler/keyup-handler state input'))
```

**Key Handlers**:
- Enter: Create new block
- Tab: Indent block
- Backspace: Merge with previous block
- Slash: Trigger command palette
- `[[`: Trigger page search
- `((`: Trigger block search
- `#`: Trigger tag search

### IME Support (International Input)

```clojure
:on-composition-start on-composition
:on-composition-update on-composition
:on-composition-end on-composition

(defn on-composition [e]
  (case e.type
    "compositionend" (do
                       (state/set-editor-in-composition! false)
                       (on-change e))
    (state/set-editor-in-composition! true)))
```

**Why Important**: Prevents processing input during multi-keystroke character composition (e.g., Japanese, Chinese)

---

## Autocomplete System

### Command Palette

**Trigger**: Type `/` in editor
**Implementation**: `commands` component (line 54-114 in `editor.cljs`)

**Flow**:
1. Detect `/` character
2. Set editor action state: `:commands`
3. Show popup with command list
4. Filter commands as user types
5. Execute command on selection

### Page/Block Search

**Triggers**:
- `[[` → Page search
- `((` → Block search
- `#` → Tag search

**Components**:
- `page-search` (line 264-287)
- `block-search` (line 350-378)
- Both use `ui/auto-complete` component

**Search Flow**:
```
User types trigger
      ↓
Extract query from cursor position
      ↓
Async search in database
      ↓
Display results in popup
      ↓
Insert selected item
```

### Popup Positioning

```clojure
(defn- open-editor-popup! [id content opts]
  (let [input (state/get-input)
        {:keys [left top rect]} (cursor/get-caret-pos input)
        pos [(+ left (:left rect) -20) (+ top (:top rect) line-height)]]
    (shui/popup-show! pos content ...)))
```

**Positioning**: Relative to cursor using `getCaretPos` utility

---

## Comparison: Logseq vs NodeSpace Current Approach

| Aspect | Logseq | NodeSpace (Current) |
|--------|--------|---------------------|
| **Edit Component** | `<textarea>` | `contenteditable` div |
| **Content State** | Plain markdown string | Mixed (originalContent + DOM) |
| **Source of Truth** | Single (textarea value) | Dual (state + innerHTML) |
| **Formatting** | View mode only | Live in contenteditable |
| **Cursor API** | Native textarea | Custom DOM range manipulation |
| **Markdown Syntax** | Always visible in edit | Partially hidden/formatted |
| **Synchronization** | None needed | Complex (state ↔ DOM) |
| **Multiline Handling** | Native textarea | Custom newline handling |
| **Auto-resize** | `react-textarea-autosize` | Custom calculation |

---

## Pros and Cons Analysis

### Logseq's Approach (Textarea + AST)

**Pros**:
1. ✅ **Simple State Management**: Single source of truth
2. ✅ **No Synchronization Bugs**: No state vs DOM conflicts
3. ✅ **Robust Cursor Handling**: Native textarea APIs
4. ✅ **Clear Separation**: Edit vs View modes are distinct
5. ✅ **Easy Multiline**: Textarea handles newlines natively
6. ✅ **Better Performance**: No complex DOM diffing during typing
7. ✅ **Easier Testing**: Pure text input/output
8. ✅ **Predictable Behavior**: Standard textarea behavior

**Cons**:
1. ❌ **No WYSIWYG**: Can't see formatting while editing
2. ❌ **Two Modes**: Context switch between edit/view
3. ❌ **Mode Toggle**: Need to click to edit, click away to save
4. ❌ **Learning Curve**: Users must know markdown syntax
5. ❌ **Complex Popup Positioning**: Requires mock textarea for accuracy
6. ❌ **Block-Level Editing**: Can't edit multiple blocks simultaneously

### NodeSpace Current Approach (Contenteditable)

**Pros**:
1. ✅ **WYSIWYG**: See formatting while typing
2. ✅ **Inline Editing**: No mode switching
3. ✅ **Familiar UX**: Like Word/Google Docs
4. ✅ **Continuous Flow**: Edit multiple paragraphs seamlessly

**Cons**:
1. ❌ **Complex State Sync**: originalContent vs innerHTML
2. ❌ **Cursor Position Bugs**: Formatting markers break positioning
3. ❌ **Fragile Implementation**: Many edge cases
4. ❌ **Multiline Formatting Loss**: Syntax lost on blur/input
5. ❌ **Hard to Test**: DOM-dependent behavior
6. ❌ **Browser Inconsistencies**: contenteditable varies by browser

---

## Migration Considerations

### If NodeSpace Adopts Logseq's Approach

#### What Changes

1. **Replace `contenteditable` with `<textarea>`**
   ```svelte
   <!-- Current -->
   <div contenteditable bind:innerHTML={content}>

   <!-- New -->
   <textarea bind:value={content} />
   ```

2. **Separate Edit and View Modes**
   ```svelte
   {#if editing}
     <textarea bind:value={content} />
   {:else}
     <MarkdownRenderer {content} />
   {/if}
   ```

3. **Simplify State Management**
   ```typescript
   // Current
   let originalContent = '...'
   let domContent = element.innerHTML
   // Sync between them constantly

   // New
   let content = '...'
   // Single source of truth
   ```

4. **Use Native Cursor APIs**
   ```typescript
   // Current
   const range = document.createRange()
   const selection = window.getSelection()
   // Complex manipulation

   // New
   textarea.selectionStart
   textarea.setSelectionRange(pos, pos)
   ```

#### Implementation Steps

1. **Phase 1: Proof of Concept**
   - Create textarea-based editor component
   - Implement basic markdown rendering
   - Test with simple content

2. **Phase 2: Feature Parity**
   - Port autocomplete/command palette
   - Implement block-level editing
   - Add keyboard shortcuts

3. **Phase 3: Polish**
   - Smooth transitions between modes
   - Popup positioning refinement
   - Performance optimization

4. **Phase 4: Migration**
   - Gradual rollout
   - User preference toggle
   - Deprecate old implementation

#### Challenges

1. **UX Change**: Users expecting WYSIWYG
   - **Mitigation**: Notion-style hybrid (inline previews)

2. **Block Model**: Logseq's block-everything approach
   - **Mitigation**: Start with paragraph-level editing

3. **Existing Content**: Migration of saved content
   - **Mitigation**: Both formats are markdown - no data migration needed

4. **Feature Parity**: Command palette, autocomplete, etc.
   - **Mitigation**: Adapt Logseq patterns incrementally

#### Hybrid Approach Option

**"Best of Both Worlds"**:
- Use textarea for editing
- Show live preview below/beside
- Notion-style inline embeds (images, links, etc.)
- Click to focus block, auto-save on blur

```
┌────────────────────────────────┐
│ Markdown Editor (textarea)     │
│ **Bold** and *italic*          │
└────────────────────────────────┘
         ↓ Live preview
┌────────────────────────────────┐
│ Rendered View                  │
│ Bold and italic                │
└────────────────────────────────┘
```

---

## Key Takeaways for NodeSpace

### 1. Architecture Decision

**Question**: Should NodeSpace switch to textarea + AST?

**Recommendation**: **Yes, with phased approach**

**Rationale**:
- Current contenteditable approach has fundamental synchronization issues
- Bugs in cursor positioning and formatting are symptoms of architectural mismatch
- Logseq's approach is battle-tested with large user base
- Simpler mental model = fewer bugs long-term

### 2. Implementation Priority

**High Priority (Core Issues)**:
1. Replace contenteditable with textarea
2. Separate edit/view modes
3. Single source of truth for content

**Medium Priority (User Experience)**:
1. Smooth edit/view transitions
2. Command palette and autocomplete
3. Keyboard shortcuts

**Low Priority (Nice-to-Have)**:
1. Live preview panel
2. Inline embeds
3. Advanced formatting toolbar

### 3. Lessons from Logseq

**What to Copy**:
- Textarea + separate rendering architecture
- Single content state (no DOM sync)
- Native cursor APIs
- Block-level edit granularity
- Command palette pattern

**What to Adapt**:
- Don't need full block-based outliner (yet)
- Can have simpler rendering (no Org-mode)
- May want hybrid preview mode
- Different keyboard shortcuts (familiar to our users)

**What to Avoid**:
- Don't try to make contenteditable work long-term
- Don't mix formatted and editable content
- Don't maintain dual state (DOM + JS)

---

## Critical Code References

### Editor Initialization
- **File**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/components/editor.cljs`
- **Lines**: 792-847
- **Component**: `box`

### Textarea Component
- **File**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/ui.cljs`
- **Lines**: 104-145
- **Component**: `ls-textarea`
- **Library**: `react-textarea-autosize` (line 7, 46)

### Markdown Rendering
- **File**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/components/block.cljs`
- **Lines**: 4208-4400+
- **Function**: `markup-element-cp`

### Cursor Utilities
- **File**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/util/cursor.cljs`
- **Functions**: `pos`, `get-caret-pos`, `move-cursor-to`, etc.

### State Management
- **File**: `/Users/malibio/Zed Projects/logseq/src/main/frontend/state.cljs`
- **Key atoms**: `state/get-edit-content`, `state/set-edit-content!`

---

## Recommended Next Steps for NodeSpace

### Immediate (This Week)
1. ✅ Complete this analysis document
2. 🔲 Create spike/prototype with textarea approach
3. 🔲 Test basic markdown editing with textarea
4. 🔲 Compare UX with current implementation

### Short-Term (Next Sprint)
1. 🔲 Design migration plan with team
2. 🔲 Create feature parity checklist
3. 🔲 Implement textarea-based editor MVP
4. 🔲 User testing with both approaches

### Long-Term (Future Sprints)
1. 🔲 Gradual rollout with feature flags
2. 🔲 Monitor user feedback
3. 🔲 Deprecate contenteditable approach
4. 🔲 Document lessons learned

---

## Conclusion

Logseq's textarea-based approach is **fundamentally simpler and more robust** than contenteditable. The key insight is:

> **Don't try to make contenteditable do markdown. Use a textarea for editing, and render markdown separately.**

This eliminates entire categories of bugs:
- No state synchronization issues
- No cursor positioning problems with formatting markers
- No content loss on blur/focus
- No multiline formatting bugs

The trade-off is a less "modern" UX (no live formatting), but this can be mitigated with:
- Quick edit/view toggle
- Live preview panel
- Notion-style inline embeds

For NodeSpace, adopting this architecture would solve current bugs and provide a more maintainable foundation for future features.

---

**Document Status**: Complete
**Confidence Level**: High (based on thorough code analysis)
**Recommended Action**: Prototype textarea approach in parallel branch
