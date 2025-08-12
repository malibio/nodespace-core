import { J as current_component, K as fallback, M as store_get, B as setContext, N as attr, E as slot, O as unsubscribe_stores, P as bind_props, D as pop, A as push, Q as attr_class, R as clsx, S as stringify, I as escape_html, T as ensure_array_like, U as copy_payload, V as assign_payload, W as attr_style, X as spread_attributes } from "../../chunks/index2.js";
import { invoke } from "@tauri-apps/api/core";
import "@tauri-apps/api/app";
import { d as derived, g as get, w as writable } from "../../chunks/index.js";
import { clsx as clsx$1 } from "clsx";
import { twMerge } from "tailwind-merge";
import { tv } from "tailwind-variants";
function onDestroy(fn) {
  var context = (
    /** @type {Component} */
    current_component
  );
  (context.d ??= []).push(fn);
}
function getResolvedTheme(theme, systemTheme2) {
  if (theme === "system") {
    return systemTheme2 || (typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  }
  return theme;
}
const themePreference = writable("system");
const systemTheme = writable("light");
const currentTheme = derived(
  [themePreference, systemTheme],
  ([$themePreference, $systemTheme]) => {
    return getResolvedTheme($themePreference, $systemTheme);
  }
);
function setTheme(theme) {
  themePreference.set(theme);
}
function toggleTheme() {
  const current = get(themePreference);
  if (current === "system") {
    const system = get(systemTheme);
    setTheme(system === "dark" ? "light" : "dark");
  } else if (current === "light") {
    setTheme("dark");
  } else {
    setTheme("light");
  }
}
function resetThemeToSystem() {
  setTheme("system");
}
function ThemeProvider($$payload, $$props) {
  push();
  var $$store_subs;
  let enableTransitions = fallback($$props["enableTransitions"], true);
  let transitionDuration = fallback($$props["transitionDuration"], 300);
  let themeContext;
  onDestroy(() => {
    const transitionStyle = document.getElementById("nodespace-theme-transitions");
    if (transitionStyle) {
      transitionStyle.remove();
    }
  });
  {
    themeContext = {
      theme: store_get($$store_subs ??= {}, "$currentTheme", currentTheme),
      preference: store_get($$store_subs ??= {}, "$themePreference", themePreference),
      setTheme,
      toggleTheme,
      resetThemeToSystem
    };
    setContext("theme", themeContext);
  }
  $$payload.out.push(`<div class="theme-provider svelte-1uxr1pi"${attr("data-theme", store_get($$store_subs ??= {}, "$currentTheme", currentTheme))}><!---->`);
  slot($$payload, $$props, "default", { themeContext }, null);
  $$payload.out.push(`<!----></div>`);
  if ($$store_subs) unsubscribe_stores($$store_subs);
  bind_props($$props, { enableTransitions, transitionDuration });
  pop();
}
const textIcon = "M160-400v-80h280v80H160Zm0-160v-80h440v80H160Zm0-160v-80h440v80H160Zm360 560v-123l221-220q9-9 20-13t22-4q12 0 23 4.5t20 13.5l37 37q8 9 12.5 20t4.5 22q0 11-4 22.5T863-380L643-160H520Zm300-263-37-37 37 37ZM580-220h38l121-122-18-19-19-18-122 121v38Zm141-141-19-18 37 37-18-19Z";
function Icon($$payload, $$props) {
  let iconPath;
  const iconRegistry = { text: textIcon };
  let name = $$props["name"];
  let size = fallback($$props["size"], 20);
  let className = fallback($$props["className"], "");
  let color = fallback($$props["color"], "currentColor");
  iconPath = iconRegistry[name];
  if (!iconPath) {
    console.warn(`Icon "${name}" not found in registry`);
  }
  $$payload.out.push(`<svg${attr("width", size)}${attr("height", size)} viewBox="0 -960 960 960"${attr("fill", color)}${attr_class(`ns-icon ns-icon--${name} ${className}`, "svelte-1yloz1g")} role="img"${attr("aria-label", `${name} icon`)}>`);
  if (iconPath) {
    $$payload.out.push("<!--[-->");
    $$payload.out.push(`<path${attr("d", iconPath)}></path>`);
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<rect x="200" y="-760" width="560" height="560" rx="40"></rect>`);
  }
  $$payload.out.push(`<!--]--></svg>`);
  bind_props($$props, { name, size, className, color });
}
class MarkdownPatternDetector {
  subscribers = [];
  lastDetectionMetrics;
  lastPatterns = [];
  constructor() {
    this.lastDetectionMetrics = this.createEmptyMetrics();
  }
  /**
   * Detect all patterns in content
   */
  detectPatterns(content, options = {}) {
    const startTime = performance.now();
    const opts = this.mergeDefaultOptions(options);
    const warnings = [];
    const patterns = [];
    try {
      const blockDetectionStart = performance.now();
      if (opts.detectHeaders) {
        patterns.push(...this.detectHeaders(content, opts));
      }
      if (opts.detectBullets) {
        patterns.push(...this.detectBullets(content, opts));
      }
      if (opts.detectBlockquotes) {
        patterns.push(...this.detectBlockquotes(content, opts));
      }
      if (opts.detectCodeBlocks) {
        patterns.push(...this.detectCodeBlocks(content, opts));
      }
      const blockDetectionEnd = performance.now();
      const inlineDetectionStart = performance.now();
      if (opts.detectBold || opts.detectItalic || opts.detectInlineCode) {
        patterns.push(...this.detectInlinePatterns(content, patterns, opts));
      }
      const inlineDetectionEnd = performance.now();
      const totalTime = performance.now() - startTime;
      patterns.sort((a, b) => a.start - b.start);
      this.lastDetectionMetrics = {
        blockDetectionTime: blockDetectionEnd - blockDetectionStart,
        inlineDetectionTime: inlineDetectionEnd - inlineDetectionStart,
        totalTime,
        regexOperations: this.countRegexOperations(opts),
        contentLength: content.length,
        patternsPerMs: patterns.length / (totalTime || 1)
      };
      if (totalTime > 50) {
        warnings.push(`Detection took ${totalTime.toFixed(2)}ms, exceeding 50ms target`);
      }
      const result = {
        patterns,
        detectionTime: totalTime,
        linesProcessed: content.split("\n").length,
        contentLength: content.length,
        warnings
      };
      this.emitPatternEvent("patterns_detected", patterns, content);
      this.lastPatterns = patterns;
      return result;
    } catch (error) {
      warnings.push(`Detection error: ${error instanceof Error ? error.message : "Unknown error"}`);
      return {
        patterns: [],
        detectionTime: performance.now() - startTime,
        linesProcessed: 0,
        contentLength: content.length,
        warnings
      };
    }
  }
  /**
   * Real-time detection with cursor information
   */
  detectPatternsRealtime(content, cursorPosition, options = {}) {
    const result = this.detectPatterns(content, options);
    const cursorInfo = this.getCursorPosition(content, cursorPosition, result.patterns);
    this.emitPatternEvent("patterns_changed", result.patterns, content, cursorInfo);
    return result;
  }
  /**
   * Detect header patterns (# ## ### #### ##### ######)
   */
  detectHeaders(content, options) {
    const patterns = [];
    const lines = content.split("\n");
    let currentPosition = 0;
    const maxLevel = options.maxHeaderLevel || 6;
    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
      if (headerMatch && headerMatch[1].length <= maxLevel) {
        const syntax = headerMatch[1];
        const headerContent = headerMatch[2];
        const level = syntax.length;
        patterns.push({
          type: "header",
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: headerContent,
          level,
          line: lineIndex,
          column: 0
        });
      }
      currentPosition += line.length + 1;
    }
    return patterns;
  }
  /**
   * Detect bullet list patterns (- * +)
   */
  detectBullets(content, options) {
    const patterns = [];
    const lines = content.split("\n");
    let currentPosition = 0;
    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const bulletMatch = line.match(/^(\s*)([-*+])\s+(.+)$/);
      if (bulletMatch && bulletMatch[3].trim().length > 0) {
        const indent = bulletMatch[1];
        const bulletChar = bulletMatch[2];
        const bulletContent = bulletMatch[3];
        const syntax = `${indent}${bulletChar} `;
        patterns.push({
          type: "bullet",
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: bulletContent,
          bulletType: bulletChar,
          line: lineIndex,
          column: indent.length
        });
      }
      currentPosition += line.length + 1;
    }
    return patterns;
  }
  /**
   * Detect blockquote patterns (> text)
   */
  detectBlockquotes(content, options) {
    const patterns = [];
    const lines = content.split("\n");
    let currentPosition = 0;
    for (let lineIndex = 0; lineIndex < lines.length; lineIndex++) {
      const line = lines[lineIndex];
      const quoteMatch = line.match(/^(\s*)(>+)\s*(.*)$/);
      if (quoteMatch) {
        const indent = quoteMatch[1];
        const quoteChars = quoteMatch[2];
        const quoteContent = quoteMatch[3];
        const syntax = `${indent}${quoteChars} `;
        patterns.push({
          type: "blockquote",
          start: currentPosition,
          end: currentPosition + line.length,
          syntax,
          content: quoteContent,
          line: lineIndex,
          column: indent.length
        });
      }
      currentPosition += line.length + 1;
    }
    return patterns;
  }
  /**
   * Detect code block patterns (```code```)
   */
  detectCodeBlocks(content, options) {
    const patterns = [];
    const codeBlockRegex = /^```(\w+)?\n?([\s\S]*?)\n?```$/gm;
    let match;
    while ((match = codeBlockRegex.exec(content)) !== null) {
      const language = match[1] || "";
      const codeContent = match[2] || "";
      const syntax = language ? `\`\`\`${language}` : "```";
      const beforeMatch = content.substring(0, match.index);
      const lines = beforeMatch.split("\n");
      const line = lines.length - 1;
      const column = lines[lines.length - 1].length;
      patterns.push({
        type: "codeblock",
        start: match.index,
        end: match.index + match[0].length,
        syntax,
        content: codeContent,
        language,
        line,
        column
      });
    }
    return patterns;
  }
  /**
   * Detect inline patterns (bold, italic, inline code)
   */
  detectInlinePatterns(content, blockPatterns, options) {
    const patterns = [];
    const processableRanges = this.getProcessableRanges(content, blockPatterns);
    for (const range of processableRanges) {
      const rangeContent = content.substring(range.start, range.end);
      const rangeOffset = range.start;
      if (options.detectBold) {
        patterns.push(...this.detectBoldInRange(rangeContent, rangeOffset));
      }
      if (options.detectItalic) {
        patterns.push(...this.detectItalicInRange(rangeContent, rangeOffset));
      }
      if (options.detectInlineCode) {
        patterns.push(...this.detectInlineCodeInRange(rangeContent, rangeOffset));
      }
    }
    return patterns;
  }
  /**
   * Detect bold patterns (**text** and __text__)
   */
  detectBoldInRange(content, offset) {
    const patterns = [];
    const doubleStar = /\*\*([^*\n]+)\*\*/g;
    let match;
    while ((match = doubleStar.exec(content)) !== null) {
      if (match[1] && match[1].trim().length > 0) {
        const position = this.getLineColumn(content, match.index);
        patterns.push({
          type: "bold",
          start: offset + match.index,
          end: offset + match.index + match[0].length,
          syntax: "**",
          content: match[1],
          line: position.line,
          column: position.column
        });
      }
    }
    const doubleUnderscore = /__([^_\n]+)__/g;
    while ((match = doubleUnderscore.exec(content)) !== null) {
      if (match[1] && match[1].trim().length > 0) {
        const position = this.getLineColumn(content, match.index);
        patterns.push({
          type: "bold",
          start: offset + match.index,
          end: offset + match.index + match[0].length,
          syntax: "__",
          content: match[1],
          line: position.line,
          column: position.column
        });
      }
    }
    return patterns;
  }
  /**
   * Detect italic patterns (*text* and _text_)
   */
  detectItalicInRange(content, offset) {
    const patterns = [];
    const singleStar = /(?<!\*)\*([^*]+)\*(?!\*)/g;
    let match;
    while ((match = singleStar.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: "italic",
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: "*",
        content: match[1],
        line: position.line,
        column: position.column
      });
    }
    const singleUnderscore = /(?<!_)_([^_]+)_(?!_)/g;
    while ((match = singleUnderscore.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: "italic",
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: "_",
        content: match[1],
        line: position.line,
        column: position.column
      });
    }
    return patterns;
  }
  /**
   * Detect inline code patterns (`code`)
   */
  detectInlineCodeInRange(content, offset) {
    const patterns = [];
    const inlineCodeRegex = /`([^`]+)`/g;
    let match;
    while ((match = inlineCodeRegex.exec(content)) !== null) {
      const position = this.getLineColumn(content, match.index);
      patterns.push({
        type: "inlinecode",
        start: offset + match.index,
        end: offset + match.index + match[0].length,
        syntax: "`",
        content: match[1],
        line: position.line,
        column: position.column
      });
    }
    return patterns;
  }
  /**
   * Get processable ranges (excluding code blocks)
   */
  getProcessableRanges(content, blockPatterns) {
    const codeBlocks = blockPatterns.filter((p) => p.type === "codeblock");
    if (codeBlocks.length === 0) {
      return [{ start: 0, end: content.length }];
    }
    const ranges = [];
    let currentPos = 0;
    for (const block of codeBlocks) {
      if (currentPos < block.start) {
        ranges.push({ start: currentPos, end: block.start });
      }
      currentPos = block.end;
    }
    if (currentPos < content.length) {
      ranges.push({ start: currentPos, end: content.length });
    }
    return ranges;
  }
  /**
   * Get line and column position for character index
   */
  getLineColumn(content, index) {
    const beforeIndex = content.substring(0, index);
    const lines = beforeIndex.split("\n");
    return {
      line: lines.length - 1,
      column: lines[lines.length - 1].length
    };
  }
  /**
   * Get pattern at specific position
   */
  getPatternAt(content, position) {
    const result = this.detectPatterns(content);
    return result.patterns.find((p) => position >= p.start && position < p.end) || null;
  }
  /**
   * Get patterns by type
   */
  getPatternsByType(content, type) {
    const result = this.detectPatterns(content);
    return result.patterns.filter((p) => p.type === type);
  }
  /**
   * Extract content from patterns
   */
  extractPatternContent(patterns) {
    return patterns.map((p) => p.content);
  }
  /**
   * Replace patterns in content
   */
  replacePatterns(content, replacements) {
    let result = content;
    const sortedReplacements = replacements.sort((a, b) => b.pattern.start - a.pattern.start);
    for (const replacement of sortedReplacements) {
      const before = result.substring(0, replacement.pattern.start);
      const after = result.substring(replacement.pattern.end);
      result = before + replacement.replacement + after;
    }
    return result;
  }
  /**
   * Validate pattern syntax
   */
  validatePattern(pattern) {
    const errors = [];
    const warnings = [];
    const suggestions = [];
    if (pattern.start >= pattern.end) {
      errors.push("Invalid pattern range: start must be less than end");
    }
    if (!pattern.content && pattern.type !== "codeblock") {
      warnings.push("Pattern has no content");
      suggestions.push("Consider removing empty pattern");
    }
    switch (pattern.type) {
      case "header":
        if (!pattern.level || pattern.level < 1 || pattern.level > 6) {
          errors.push("Header level must be between 1 and 6");
        }
        break;
      case "bullet":
        if (!pattern.bulletType || !["*", "-", "+"].includes(pattern.bulletType)) {
          errors.push("Invalid bullet type, must be *, -, or +");
        }
        break;
      case "codeblock":
        if (pattern.language && !/^[a-zA-Z][a-zA-Z0-9-]*$/.test(pattern.language)) {
          warnings.push("Language specification contains unusual characters");
        }
        break;
    }
    return {
      isValid: errors.length === 0,
      errors,
      warnings,
      suggestions
    };
  }
  /**
   * Get performance metrics
   */
  getMetrics() {
    return { ...this.lastDetectionMetrics };
  }
  /**
   * Subscribe to pattern detection events
   */
  subscribe(callback) {
    this.subscribers.push(callback);
    return () => {
      const index = this.subscribers.indexOf(callback);
      if (index > -1) {
        this.subscribers.splice(index, 1);
      }
    };
  }
  /**
   * Helper methods
   */
  mergeDefaultOptions(options) {
    return {
      detectHeaders: true,
      detectBullets: true,
      detectBlockquotes: true,
      detectCodeBlocks: true,
      detectBold: true,
      detectItalic: true,
      detectInlineCode: true,
      maxHeaderLevel: 6,
      includePositions: true,
      performanceMode: false,
      ...options
    };
  }
  createEmptyMetrics() {
    return {
      blockDetectionTime: 0,
      inlineDetectionTime: 0,
      totalTime: 0,
      regexOperations: 0,
      contentLength: 0,
      patternsPerMs: 0
    };
  }
  countRegexOperations(options) {
    let count = 0;
    if (options.detectHeaders) count++;
    if (options.detectBullets) count++;
    if (options.detectBlockquotes) count++;
    if (options.detectCodeBlocks) count++;
    if (options.detectBold) count += 2;
    if (options.detectItalic) count += 2;
    if (options.detectInlineCode) count++;
    return count;
  }
  getCursorPosition(content, position, patterns) {
    const lineCol = this.getLineColumn(content, position);
    const currentPattern = patterns.find((p) => position >= p.start && position < p.end);
    return {
      position,
      line: lineCol.line,
      column: lineCol.column,
      atPatternStart: currentPattern?.start === position,
      atPatternEnd: currentPattern?.end === position,
      currentPattern
    };
  }
  emitPatternEvent(type, patterns, content, cursorPosition) {
    const event = {
      type,
      patterns,
      content,
      cursorPosition,
      timestamp: Date.now()
    };
    if (type === "patterns_changed") {
      const addedPatterns = patterns.filter(
        (p) => !this.lastPatterns.some((lp) => this.patternsEqual(p, lp))
      );
      const removedPatterns = this.lastPatterns.filter(
        (lp) => !patterns.some((p) => this.patternsEqual(p, lp))
      );
      event.addedPatterns = addedPatterns;
      event.removedPatterns = removedPatterns;
    }
    this.subscribers.forEach((callback) => {
      try {
        callback(event);
      } catch (error) {
        console.warn("Pattern detection event callback error:", error);
      }
    });
  }
  patternsEqual(a, b) {
    return a.type === b.type && a.start === b.start && a.end === b.end && a.content === b.content && a.syntax === b.syntax;
  }
}
const markdownPatternDetector = new MarkdownPatternDetector();
class PatternIntegrationUtilities {
  /**
   * Convert patterns to CSS classes for WYSIWYG rendering
   */
  toCSSClasses(patterns) {
    const classMap = {};
    for (const pattern of patterns) {
      for (let pos = pattern.start; pos < pattern.end; pos++) {
        if (!classMap[pos]) {
          classMap[pos] = [];
        }
        switch (pattern.type) {
          case "header":
            classMap[pos].push(`markdown-header`, `markdown-header-${pattern.level}`);
            break;
          case "bold":
            classMap[pos].push("markdown-bold");
            break;
          case "italic":
            classMap[pos].push("markdown-italic");
            break;
          case "inlinecode":
            classMap[pos].push("markdown-inline-code");
            break;
          case "codeblock":
            classMap[pos].push("markdown-code-block");
            if (pattern.language) {
              classMap[pos].push(`markdown-code-${pattern.language}`);
            }
            break;
          case "bullet":
            classMap[pos].push("markdown-bullet", `markdown-bullet-${pattern.bulletType}`);
            break;
          case "blockquote":
            classMap[pos].push("markdown-blockquote");
            break;
        }
        if (this.isSyntaxCharacter(pos, pattern)) {
          classMap[pos].push("markdown-syntax", "markdown-syntax-hidden");
        }
      }
    }
    return classMap;
  }
  /**
   * Convert patterns to HTML structure for rendering
   */
  toHTMLStructure(content, patterns) {
    const container = document.createElement("div");
    container.className = "markdown-content";
    if (patterns.length === 0) {
      container.textContent = content;
      return container;
    }
    let currentPos = 0;
    const sortedPatterns = [...patterns].sort((a, b) => a.start - b.start);
    for (const pattern of sortedPatterns) {
      if (currentPos < pattern.start) {
        const textNode = document.createTextNode(content.substring(currentPos, pattern.start));
        container.appendChild(textNode);
      }
      const patternElement = this.createPatternElement(pattern);
      container.appendChild(patternElement);
      currentPos = pattern.end;
    }
    if (currentPos < content.length) {
      const textNode = document.createTextNode(content.substring(currentPos));
      container.appendChild(textNode);
    }
    return container;
  }
  /**
   * Handle cursor positioning around patterns
   */
  adjustCursorForPatterns(content, position, patterns) {
    const pattern = patterns.find((p) => position >= p.start && position < p.end);
    if (!pattern) {
      return position;
    }
    const syntaxLength = this.getSyntaxLength(pattern);
    if (position < pattern.start + syntaxLength) {
      return pattern.start + syntaxLength;
    }
    if (position > pattern.end - syntaxLength) {
      return pattern.end - syntaxLength;
    }
    return position;
  }
  /**
   * Extract bullet patterns for node conversion
   */
  extractBulletPatterns(patterns) {
    return patterns.filter((p) => p.type === "bullet").sort((a, b) => a.start - b.start);
  }
  /**
   * Detect soft newline context for better line handling
   */
  detectSoftNewlineContext(content, position, patterns) {
    const lineStart = content.lastIndexOf("\n", position - 1) + 1;
    const lineEnd = content.indexOf("\n", position);
    const actualLineEnd = lineEnd === -1 ? content.length : lineEnd;
    const currentLine = content.substring(lineStart, actualLineEnd);
    const currentPattern = patterns.find((p) => position >= p.start && position <= p.end);
    if (currentPattern) {
      switch (currentPattern.type) {
        case "codeblock":
        case "blockquote":
          return true;
        case "bullet":
          return position > currentPattern.start + currentPattern.syntax.length;
        default:
          return false;
      }
    }
    const trimmedLine = currentLine.trim();
    return trimmedLine.startsWith(">") || trimmedLine.match(/^[\s]*[-*+]\s/) !== null || trimmedLine.startsWith("```");
  }
  /**
   * Private helper methods
   */
  isSyntaxCharacter(position, pattern) {
    const syntaxLength = this.getSyntaxLength(pattern);
    if (position < pattern.start + syntaxLength) {
      return true;
    }
    if (this.hasClosingSyntax(pattern) && position >= pattern.end - syntaxLength) {
      return true;
    }
    return false;
  }
  getSyntaxLength(pattern) {
    switch (pattern.type) {
      case "header":
        return pattern.syntax.length + 1;
      // # + space
      case "bold":
        return pattern.syntax.length;
      // ** or __
      case "italic":
        return pattern.syntax.length;
      // * or _
      case "inlinecode":
        return 1;
      // `
      case "bullet":
        return pattern.syntax.length;
      // - or * or + (with space and indent)
      case "blockquote":
        return pattern.syntax.length;
      // > (with space)
      case "codeblock":
        return 3;
      // ```
      default:
        return 0;
    }
  }
  hasClosingSyntax(pattern) {
    return ["bold", "italic", "inlinecode", "codeblock"].includes(pattern.type);
  }
  createPatternElement(pattern) {
    let element;
    switch (pattern.type) {
      case "header":
        element = document.createElement(`h${pattern.level}`);
        element.className = `markdown-header markdown-header-${pattern.level}`;
        break;
      case "bold":
        element = document.createElement("strong");
        element.className = "markdown-bold";
        break;
      case "italic":
        element = document.createElement("em");
        element.className = "markdown-italic";
        break;
      case "inlinecode":
        element = document.createElement("code");
        element.className = "markdown-inline-code";
        break;
      case "codeblock":
        element = document.createElement("pre");
        const codeElement = document.createElement("code");
        if (pattern.language) {
          codeElement.className = `language-${pattern.language}`;
        }
        codeElement.textContent = pattern.content;
        element.appendChild(codeElement);
        element.className = "markdown-code-block";
        return element;
      case "bullet":
        element = document.createElement("li");
        element.className = `markdown-bullet markdown-bullet-${pattern.bulletType}`;
        break;
      case "blockquote":
        element = document.createElement("blockquote");
        element.className = "markdown-blockquote";
        break;
      default:
        element = document.createElement("span");
        element.className = "markdown-unknown";
    }
    element.textContent = pattern.content;
    return element;
  }
}
const patternIntegrationUtils = new PatternIntegrationUtilities();
class SoftNewlineProcessor {
  constructor(options = {}) {
    this.options = options;
    this.options = {
      minContentLength: 2,
      debounceTime: 300,
      autoCreateNodes: false,
      patternCompletionTimeout: 2e3,
      ...options
    };
  }
  lastDetection = null;
  processingTimeouts = /* @__PURE__ */ new Map();
  eventCallbacks = /* @__PURE__ */ new Set();
  /**
   * Detect Shift-Enter in keyboard events
   */
  isShiftEnter(event) {
    return event.key === "Enter" && event.shiftKey && !event.ctrlKey && !event.metaKey;
  }
  /**
   * Process content after a soft newline to detect markdown patterns
   */
  processSoftNewlineContent(content, cursorPosition, nodeId = "default") {
    const contentBeforeCursor = content.substring(0, cursorPosition);
    const lastNewlineIndex = contentBeforeCursor.lastIndexOf("\n");
    if (lastNewlineIndex === -1) {
      return this.createEmptyContext(content, cursorPosition);
    }
    const contentBefore = content.substring(0, lastNewlineIndex);
    const contentAfter = content.substring(lastNewlineIndex + 1);
    if (contentAfter.trim().length < (this.options.minContentLength || 2)) {
      return this.createEmptyContext(content, cursorPosition, lastNewlineIndex);
    }
    const patternResult = markdownPatternDetector.detectPatternsRealtime(
      contentAfter,
      cursorPosition - lastNewlineIndex - 1
    );
    const relevantPattern = this.findRelevantPattern(contentAfter, patternResult.patterns);
    const context = {
      hasMarkdownAfterNewline: relevantPattern !== null,
      detectedPattern: relevantPattern || void 0,
      contentBefore,
      contentAfter,
      newlinePosition: lastNewlineIndex,
      shouldCreateNewNode: relevantPattern !== null && this.shouldCreateNodeForPattern(relevantPattern),
      suggestedNodeType: relevantPattern ? this.getNodeTypeForPattern(relevantPattern) : void 0
    };
    this.lastDetection = context;
    this.emitContextChange(context);
    if (context.shouldCreateNewNode && this.options.autoCreateNodes) {
      this.scheduleNodeCreation(context, nodeId);
    }
    return context;
  }
  /**
   * Get node creation suggestion for detected pattern
   */
  getNodeCreationSuggestion(context) {
    if (!context.detectedPattern || !context.shouldCreateNewNode) {
      return null;
    }
    const pattern = context.detectedPattern;
    const nodeType = this.getNodeTypeForPattern(pattern);
    const cleanContent = this.extractCleanContent(pattern);
    return {
      nodeType,
      content: cleanContent,
      rawContent: context.contentAfter,
      triggerPattern: pattern,
      insertPosition: context.newlinePosition + 1,
      relationship: this.getNodeRelationship(pattern)
    };
  }
  /**
   * Handle real-time typing after soft newline with debouncing
   */
  processRealtimeTyping(content, cursorPosition, nodeId = "default") {
    return new Promise((resolve) => {
      const existingTimeout = this.processingTimeouts.get(nodeId);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }
      const timeout = setTimeout(() => {
        const context = this.processSoftNewlineContent(content, cursorPosition, nodeId);
        this.processingTimeouts.delete(nodeId);
        resolve(context);
      }, this.options.debounceTime);
      this.processingTimeouts.set(nodeId, timeout);
    });
  }
  /**
   * Cancel processing for a specific node (cleanup)
   */
  cancelProcessing(nodeId) {
    const timeout = this.processingTimeouts.get(nodeId);
    if (timeout) {
      clearTimeout(timeout);
      this.processingTimeouts.delete(nodeId);
    }
  }
  /**
   * Subscribe to soft newline context changes
   */
  subscribe(callback) {
    this.eventCallbacks.add(callback);
    return () => {
      this.eventCallbacks.delete(callback);
    };
  }
  /**
   * Get the last detected context (useful for testing)
   */
  getLastContext() {
    return this.lastDetection;
  }
  /**
   * Private helper methods
   */
  createEmptyContext(content, cursorPosition, newlinePosition) {
    return {
      hasMarkdownAfterNewline: false,
      contentBefore: newlinePosition !== void 0 ? content.substring(0, newlinePosition) : content,
      contentAfter: newlinePosition !== void 0 ? content.substring(newlinePosition + 1) : "",
      newlinePosition: newlinePosition || -1,
      shouldCreateNewNode: false
    };
  }
  findRelevantPattern(content, patterns) {
    if (patterns.length === 0) {
      return null;
    }
    const earlyPatterns = patterns.filter((p) => p.start <= 3);
    if (earlyPatterns.length === 0) {
      return null;
    }
    const blockPatterns = earlyPatterns.filter(
      (p) => ["header", "bullet", "blockquote", "codeblock"].includes(p.type)
    );
    if (blockPatterns.length > 0) {
      return blockPatterns[0];
    }
    return earlyPatterns[0];
  }
  shouldCreateNodeForPattern(pattern) {
    const blockPatterns = ["header", "bullet", "blockquote"];
    return blockPatterns.includes(pattern.type);
  }
  getNodeTypeForPattern(pattern) {
    switch (pattern.type) {
      case "header":
        return "text";
      // Headers become text nodes with special styling
      case "bullet":
        const content = pattern.content.toLowerCase();
        if (content.includes("todo") || content.includes("task") || content.includes("do ")) {
          return "task";
        }
        if (content.includes("ask") || content.includes("question") || content.includes("?")) {
          return "ai-chat";
        }
        if (content.includes("@") || content.includes("person") || content.includes("contact")) {
          return "entity";
        }
        if (content.includes("search") || content.includes("find") || content.includes("query")) {
          return "query";
        }
        return "text";
      case "blockquote":
        return "ai-chat";
      // Quotes often represent conversations or AI interactions
      case "codeblock":
        return "text";
      // Code becomes text node with code styling
      default:
        return "text";
    }
  }
  extractCleanContent(pattern) {
    return pattern.content.trim();
  }
  getNodeRelationship(pattern) {
    return pattern.type === "header" ? "sibling" : "child";
  }
  scheduleNodeCreation(context, nodeId) {
    const suggestion = this.getNodeCreationSuggestion(context);
    if (suggestion) {
      this.emitNodeCreationSuggestion(suggestion, nodeId);
    }
  }
  emitContextChange(context) {
    this.eventCallbacks.forEach((callback) => {
      try {
        callback(context);
      } catch (error) {
        console.warn("Soft newline context callback error:", error);
      }
    });
  }
  emitNodeCreationSuggestion(suggestion, nodeId) {
    const event = new CustomEvent("nodespace:node-creation-suggestion", {
      detail: { suggestion, sourceNodeId: nodeId }
    });
    document.dispatchEvent(event);
  }
}
const softNewlineProcessor = new SoftNewlineProcessor();
class WYSIWYGProcessor {
  config;
  lastProcessingTime = 0;
  debounceTimeout = null;
  subscribers = [];
  isProcessing = false;
  constructor(config = {}) {
    this.config = {
      enableRealTime: true,
      performanceMode: false,
      maxProcessingTime: 50,
      debounceDelay: 16,
      hideSyntax: true,
      enableFormatting: true,
      cssPrefix: "wysiwyg",
      ...config
    };
  }
  /**
   * Process content for WYSIWYG display
   */
  async process(content, cursorPosition, options = {}) {
    const startTime = performance.now();
    const warnings = [];
    try {
      this.isProcessing = true;
      const detectionOptions = {
        performanceMode: this.config.performanceMode,
        ...options
      };
      const detectionResult = cursorPosition !== void 0 ? markdownPatternDetector.detectPatternsRealtime(content, cursorPosition, detectionOptions) : markdownPatternDetector.detectPatterns(content, detectionOptions);
      warnings.push(...detectionResult.warnings);
      const characterClasses = this.config.enableFormatting ? patternIntegrationUtils.toCSSClasses(detectionResult.patterns) : {};
      const processedHTML = await this.generateProcessedHTML(content, detectionResult.patterns);
      let adjustedCursorPosition = cursorPosition;
      if (cursorPosition !== void 0) {
        adjustedCursorPosition = patternIntegrationUtils.adjustCursorForPatterns(
          content,
          cursorPosition,
          detectionResult.patterns
        );
      }
      const processingTime = performance.now() - startTime;
      this.lastProcessingTime = processingTime;
      if (processingTime > this.config.maxProcessingTime) {
        warnings.push(`WYSIWYG processing took ${processingTime.toFixed(2)}ms, exceeding ${this.config.maxProcessingTime}ms target`);
      }
      const result = {
        originalContent: content,
        processedHTML,
        characterClasses: this.addWYSIWYGClasses(characterClasses, detectionResult.patterns),
        patterns: detectionResult.patterns,
        processingTime,
        adjustedCursorPosition,
        warnings
      };
      this.emitEvent({
        type: "processed",
        result,
        timestamp: Date.now()
      });
      return result;
    } catch (error) {
      const processingTime = performance.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : "Unknown processing error";
      warnings.push(`Processing error: ${errorMessage}`);
      this.emitEvent({
        type: "error",
        error: errorMessage,
        timestamp: Date.now()
      });
      return {
        originalContent: content,
        processedHTML: this.escapeHTML(content),
        characterClasses: {},
        patterns: [],
        processingTime,
        warnings
      };
    } finally {
      this.isProcessing = false;
    }
  }
  /**
   * Process content with debouncing for real-time typing
   */
  processRealTime(content, cursorPosition, callback) {
    if (!this.config.enableRealTime) {
      return;
    }
    if (this.debounceTimeout !== null) {
      clearTimeout(this.debounceTimeout);
    }
    const adaptiveDelay = this.lastProcessingTime > this.config.maxProcessingTime ? this.config.debounceDelay * 2 : this.config.debounceDelay;
    this.debounceTimeout = setTimeout(async () => {
      try {
        const result = await this.process(content, cursorPosition);
        callback(result);
      } catch (error) {
        console.warn("Real-time WYSIWYG processing error:", error);
      }
      this.debounceTimeout = null;
    }, adaptiveDelay);
  }
  /**
   * Generate processed HTML with hidden syntax and formatting
   */
  async generateProcessedHTML(content, patterns) {
    if (!this.config.hideSyntax && !this.config.enableFormatting) {
      return this.escapeHTML(content);
    }
    const sortedPatterns = [...patterns].sort((a, b) => a.start - b.start);
    let result = "";
    let currentPos = 0;
    for (const pattern of sortedPatterns) {
      if (currentPos < pattern.start) {
        result += this.escapeHTML(content.substring(currentPos, pattern.start));
      }
      result += await this.processPattern(pattern, content.substring(pattern.start, pattern.end));
      currentPos = pattern.end;
    }
    if (currentPos < content.length) {
      result += this.escapeHTML(content.substring(currentPos));
    }
    return result;
  }
  /**
   * Process individual pattern for WYSIWYG display
   */
  async processPattern(pattern, originalText) {
    const cssClasses = this.getPatternCSSClasses(pattern);
    const escapedContent = this.escapeHTML(pattern.content);
    switch (pattern.type) {
      case "header":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)} ">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "bold":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="${this.escapeHTML(pattern.syntax)}" data-syntax-after="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "italic":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="${this.escapeHTML(pattern.syntax)}" data-syntax-after="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "inlinecode":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax-before="\`" data-syntax-after="\`">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "bullet":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "blockquote":
        if (this.config.hideSyntax) {
          return `<span class="${cssClasses}" data-syntax="${this.escapeHTML(pattern.syntax)}">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      case "codeblock":
        if (this.config.hideSyntax) {
          const openSyntax = pattern.language ? `\`\`\`${pattern.language}` : "```";
          return `<span class="${cssClasses}" data-syntax-before="${openSyntax}" data-syntax-after="\`\`\`">${escapedContent}</span>`;
        } else {
          return `<span class="${cssClasses}">${this.escapeHTML(originalText)}</span>`;
        }
      default:
        return this.escapeHTML(originalText);
    }
  }
  /**
   * Get CSS classes for pattern
   */
  getPatternCSSClasses(pattern) {
    const classes = [`${this.config.cssPrefix}-${pattern.type}`];
    switch (pattern.type) {
      case "header":
        classes.push(`${this.config.cssPrefix}-header-${pattern.level}`);
        break;
      case "bullet":
        classes.push(`${this.config.cssPrefix}-bullet-${pattern.bulletType}`);
        break;
      case "codeblock":
        if (pattern.language) {
          classes.push(`${this.config.cssPrefix}-code-${pattern.language}`);
        }
        break;
    }
    if (this.config.hideSyntax) {
      classes.push(`${this.config.cssPrefix}-syntax-hidden`);
    }
    return classes.join(" ");
  }
  /**
   * Add WYSIWYG-specific CSS classes to character class map
   */
  addWYSIWYGClasses(characterClasses, patterns) {
    const result = { ...characterClasses };
    for (const pattern of patterns) {
      for (let pos = pattern.start; pos < pattern.end; pos++) {
        if (!result[pos]) {
          result[pos] = [];
        }
        const wysiwygClasses = result[pos].map((cls) => `${this.config.cssPrefix}-${cls}`);
        result[pos] = [...result[pos], ...wysiwygClasses];
        if (this.config.hideSyntax && this.isSyntaxPosition(pos, pattern)) {
          result[pos].push(`${this.config.cssPrefix}-syntax`, `${this.config.cssPrefix}-syntax-hidden`);
        }
      }
    }
    return result;
  }
  /**
   * Check if position is a syntax character
   */
  isSyntaxPosition(position, pattern) {
    const syntaxLength = this.getSyntaxLength(pattern);
    if (position < pattern.start + syntaxLength) {
      return true;
    }
    if (this.hasClosingSyntax(pattern) && position >= pattern.end - syntaxLength) {
      return true;
    }
    return false;
  }
  /**
   * Get syntax length for pattern
   */
  getSyntaxLength(pattern) {
    switch (pattern.type) {
      case "header":
        return pattern.syntax.length + 1;
      // # + space
      case "bold":
      case "italic":
        return pattern.syntax.length;
      case "inlinecode":
        return 1;
      case "bullet":
        return pattern.syntax.length;
      case "blockquote":
        return pattern.syntax.length;
      case "codeblock":
        return 3;
      // ```
      default:
        return 0;
    }
  }
  /**
   * Check if pattern has closing syntax
   */
  hasClosingSyntax(pattern) {
    return ["bold", "italic", "inlinecode", "codeblock"].includes(pattern.type);
  }
  /**
   * Escape HTML characters
   */
  escapeHTML(text) {
    if (!text || typeof text !== "string") {
      return "";
    }
    const escapeMap = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;"
    };
    return text.replace(/[&<>"']/g, (char) => escapeMap[char]);
  }
  /**
   * Subscribe to WYSIWYG processing events
   */
  subscribe(callback) {
    this.subscribers.push(callback);
    return () => {
      const index = this.subscribers.indexOf(callback);
      if (index > -1) {
        this.subscribers.splice(index, 1);
      }
    };
  }
  /**
   * Get current configuration
   */
  getConfig() {
    return { ...this.config };
  }
  /**
   * Update configuration
   */
  updateConfig(newConfig) {
    this.config = { ...this.config, ...newConfig };
  }
  /**
   * Get processing metrics
   */
  getMetrics() {
    return {
      lastProcessingTime: this.lastProcessingTime,
      isProcessing: this.isProcessing,
      averageProcessingTime: this.lastProcessingTime
      // TODO: Track average over time
    };
  }
  /**
   * Clear debounce timeout
   */
  cancelPendingProcessing() {
    if (this.debounceTimeout !== null) {
      clearTimeout(this.debounceTimeout);
      this.debounceTimeout = null;
    }
  }
  /**
   * Emit processing event
   */
  emitEvent(event) {
    this.subscribers.forEach((callback) => {
      try {
        callback(event);
      } catch (error) {
        console.warn("WYSIWYG event callback error:", error);
      }
    });
  }
}
new WYSIWYGProcessor();
function BaseNode($$payload, $$props) {
  push();
  let nodeClasses, displayIcon, iconAnimationClass;
  let nodeType = fallback($$props["nodeType"], "text");
  let nodeId = fallback($$props["nodeId"], "");
  let content = fallback($$props["content"], "");
  let hasChildren = fallback($$props["hasChildren"], false);
  let className = fallback($$props["className"], "");
  let editable = fallback($$props["editable"], true);
  let contentEditable = fallback($$props["contentEditable"], true);
  let multiline = fallback($$props["multiline"], false);
  let placeholder = fallback($$props["placeholder"], "Click to add content...");
  let iconName = fallback($$props["iconName"], void 0);
  let isProcessing = fallback($$props["isProcessing"], false);
  let processingAnimation = fallback($$props["processingAnimation"], "pulse");
  let processingIcon = fallback($$props["processingIcon"], void 0);
  let enableWYSIWYG = fallback($$props["enableWYSIWYG"], true);
  let wysiwygConfig = fallback(
    $$props["wysiwygConfig"],
    () => ({
      enableRealTime: true,
      performanceMode: false,
      maxProcessingTime: 50,
      debounceDelay: 16,
      hideSyntax: true,
      enableFormatting: true
    }),
    true
  );
  function getNodeTypeLabel(type) {
    switch (type) {
      case "text":
        return "Text";
      case "task":
        return "Task";
      case "ai-chat":
        return "AI Chat";
      case "entity":
        return "Entity";
      case "query":
        return "Query";
      default:
        return "Node";
    }
  }
  onDestroy(() => {
    softNewlineProcessor.cancelProcessing(nodeId);
  });
  nodeClasses = [
    "ns-node",
    `ns-node--${nodeType}`,
    hasChildren && "ns-node--has-children",
    isProcessing && "ns-node--processing",
    className
  ].filter(Boolean).join(" ");
  displayIcon = isProcessing && processingIcon ? processingIcon : iconName;
  iconAnimationClass = isProcessing ? `ns-node__icon--${processingAnimation}` : "";
  $$payload.out.push(`<div${attr_class(clsx(nodeClasses), "svelte-1tgptln")} role="button" tabindex="0"${attr("aria-label", `${stringify(getNodeTypeLabel(nodeType))}: ${stringify(content || "Empty node")}`)}${attr("data-node-id", nodeId)}${attr("data-node-type", nodeType)}><header class="ns-node__header svelte-1tgptln"><div class="ns-node__indicator svelte-1tgptln"${attr("data-node-type", nodeType)}>`);
  if (displayIcon) {
    $$payload.out.push("<!--[-->");
    Icon($$payload, {
      name: displayIcon,
      size: 16,
      className: `ns-node__icon ${stringify(iconAnimationClass)}`
    });
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<div${attr_class(`ns-node__circle ${stringify(iconAnimationClass)}`, "svelte-1tgptln", { "ns-node__circle--has-children": hasChildren })}></div>`);
  }
  $$payload.out.push(`<!--]--></div> <div class="ns-node__content svelte-1tgptln"><!---->`);
  slot($$payload, $$props, "content", {}, () => {
    {
      $$payload.out.push("<!--[!-->");
      if (editable && contentEditable) {
        $$payload.out.push("<!--[-->");
        $$payload.out.push(`<div class="ns-node__display ns-node__display--clickable svelte-1tgptln" role="button" tabindex="0" aria-label="Click to edit content"><!---->`);
        slot($$payload, $$props, "display-content", {}, () => {
          if (content) {
            $$payload.out.push("<!--[-->");
            $$payload.out.push(`<span${attr_class(`ns-node__text ${stringify(enableWYSIWYG ? "ns-node__text--wysiwyg" : "")}`, "svelte-1tgptln")}>`);
            {
              $$payload.out.push("<!--[!-->");
              $$payload.out.push(`${escape_html(content)}`);
            }
            $$payload.out.push(`<!--]--></span>`);
          } else {
            $$payload.out.push("<!--[!-->");
            $$payload.out.push(`<span class="ns-node__empty svelte-1tgptln">${escape_html(placeholder)}</span>`);
          }
          $$payload.out.push(`<!--]-->`);
        });
        $$payload.out.push(`<!----></div>`);
      } else {
        $$payload.out.push("<!--[!-->");
        $$payload.out.push(`<div class="ns-node__display svelte-1tgptln" role="region"><!---->`);
        slot($$payload, $$props, "display-content", {}, () => {
          if (content) {
            $$payload.out.push("<!--[-->");
            $$payload.out.push(`<span${attr_class(`ns-node__text ${stringify(enableWYSIWYG ? "ns-node__text--wysiwyg" : "")}`, "svelte-1tgptln")}>`);
            {
              $$payload.out.push("<!--[!-->");
              $$payload.out.push(`${escape_html(content)}`);
            }
            $$payload.out.push(`<!--]--></span>`);
          } else {
            $$payload.out.push("<!--[!-->");
            $$payload.out.push(`<span class="ns-node__empty svelte-1tgptln">${escape_html(placeholder)}</span>`);
          }
          $$payload.out.push(`<!--]-->`);
        });
        $$payload.out.push(`<!----></div>`);
      }
      $$payload.out.push(`<!--]-->`);
    }
    $$payload.out.push(`<!--]-->`);
  });
  $$payload.out.push(`<!----> <!---->`);
  slot($$payload, $$props, "default", {}, null);
  $$payload.out.push(`<!----></div></header></div>`);
  bind_props($$props, {
    nodeType,
    nodeId,
    content,
    hasChildren,
    className,
    editable,
    contentEditable,
    multiline,
    placeholder,
    iconName,
    isProcessing,
    processingAnimation,
    processingIcon,
    enableWYSIWYG,
    wysiwygConfig
  });
  pop();
}
function MarkdownRenderer($$payload, $$props) {
  push();
  let finalNodes;
  let content = fallback($$props["content"], "");
  let nodes = fallback($$props["nodes"], () => [], true);
  function parseMarkdownSafe(markdown) {
    if (!markdown) return [];
    const paragraphs = markdown.split(/\n\s*\n/);
    const result = [];
    for (const para of paragraphs) {
      if (!para.trim()) continue;
      const headingMatch = para.match(/^(#{1,6})\s+(.+)$/);
      if (headingMatch) {
        const level = headingMatch[1].length;
        const headingContent = headingMatch[2].trim();
        result.push({
          type: "heading",
          level,
          children: parseInlineContent(headingContent),
          className: `ns-markdown-heading ns-markdown-h${level}`
        });
      } else {
        const paragraphContent = para.replace(/\n/g, "\n");
        result.push({
          type: "paragraph",
          children: parseInlineContent(paragraphContent),
          className: "ns-markdown-paragraph"
        });
      }
    }
    return result;
  }
  function parseInlineContent(text) {
    const result = [];
    let remaining = text;
    while (remaining.length > 0) {
      const brMatch = remaining.match(/^(.+?)\n/);
      if (brMatch) {
        const beforeBr = brMatch[1];
        if (beforeBr) {
          result.push(...parseFormattingNodes(beforeBr));
        }
        result.push({ type: "br" });
        remaining = remaining.slice(brMatch[0].length);
        continue;
      }
      result.push(...parseFormattingNodes(remaining));
      break;
    }
    return result;
  }
  function parseFormattingNodes(text) {
    const result = [];
    let remaining = text;
    while (remaining.length > 0) {
      const boldMatch = remaining.match(/^(.*?)\*\*(.*?)\*\*(.*)/s) || remaining.match(/^(.*?)__(.*?)__(.*)/s);
      if (boldMatch) {
        const [, before, boldText, after] = boldMatch;
        if (before) result.push({ type: "text", content: before });
        result.push({
          type: "bold",
          children: [{ type: "text", content: boldText }],
          className: "ns-markdown-bold"
        });
        remaining = after;
        continue;
      }
      const italicMatch = remaining.match(/^(.*?)(?<!\*)\*(?!\*)([^*]+)\*(?!\*)(.*)/s) || remaining.match(/^(.*?)(?<!_)_(?!_)([^_]+)_(?!_)(.*)/s);
      if (italicMatch) {
        const [, before, italicText, after] = italicMatch;
        if (before) result.push({ type: "text", content: before });
        result.push({
          type: "italic",
          children: [{ type: "text", content: italicText }],
          className: "ns-markdown-italic"
        });
        remaining = after;
        continue;
      }
      const codeMatch = remaining.match(/^(.*?)`([^`]+)`(.*)/s);
      if (codeMatch) {
        const [, before, codeText, after] = codeMatch;
        if (before) result.push({ type: "text", content: before });
        result.push({
          type: "code",
          content: codeText,
          className: "ns-markdown-code"
        });
        remaining = after;
        continue;
      }
      const linkMatch = remaining.match(/^(.*?)\[([^\]]+)\]\(([^)]+)\)(.*)/s);
      if (linkMatch) {
        const [, before, linkText, url, after] = linkMatch;
        if (before) result.push({ type: "text", content: before });
        result.push({
          type: "link",
          href: url,
          children: [{ type: "text", content: linkText }],
          className: "ns-markdown-link"
        });
        remaining = after;
        continue;
      }
      result.push({ type: "text", content: remaining });
      break;
    }
    return result;
  }
  finalNodes = nodes.length > 0 ? nodes : parseMarkdownSafe(content);
  const each_array = ensure_array_like(finalNodes);
  $$payload.out.push(`<!--[-->`);
  for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
    let node = each_array[$$index];
    if (node.type === "paragraph") {
      $$payload.out.push("<!--[-->");
      $$payload.out.push(`<p${attr_class(clsx(node.className || ""))}>`);
      MarkdownRenderer($$payload, { nodes: node.children || [] });
      $$payload.out.push(`<!----></p>`);
    } else {
      $$payload.out.push("<!--[!-->");
      if (node.type === "heading") {
        $$payload.out.push("<!--[-->");
        if (node.level === 1) {
          $$payload.out.push("<!--[-->");
          $$payload.out.push(`<h1${attr_class(clsx(node.className || ""))}>`);
          MarkdownRenderer($$payload, { nodes: node.children || [] });
          $$payload.out.push(`<!----></h1>`);
        } else {
          $$payload.out.push("<!--[!-->");
          if (node.level === 2) {
            $$payload.out.push("<!--[-->");
            $$payload.out.push(`<h2${attr_class(clsx(node.className || ""))}>`);
            MarkdownRenderer($$payload, { nodes: node.children || [] });
            $$payload.out.push(`<!----></h2>`);
          } else {
            $$payload.out.push("<!--[!-->");
            if (node.level === 3) {
              $$payload.out.push("<!--[-->");
              $$payload.out.push(`<h3${attr_class(clsx(node.className || ""))}>`);
              MarkdownRenderer($$payload, { nodes: node.children || [] });
              $$payload.out.push(`<!----></h3>`);
            } else {
              $$payload.out.push("<!--[!-->");
              if (node.level === 4) {
                $$payload.out.push("<!--[-->");
                $$payload.out.push(`<h4${attr_class(clsx(node.className || ""))}>`);
                MarkdownRenderer($$payload, { nodes: node.children || [] });
                $$payload.out.push(`<!----></h4>`);
              } else {
                $$payload.out.push("<!--[!-->");
                if (node.level === 5) {
                  $$payload.out.push("<!--[-->");
                  $$payload.out.push(`<h5${attr_class(clsx(node.className || ""))}>`);
                  MarkdownRenderer($$payload, { nodes: node.children || [] });
                  $$payload.out.push(`<!----></h5>`);
                } else {
                  $$payload.out.push("<!--[!-->");
                  if (node.level === 6) {
                    $$payload.out.push("<!--[-->");
                    $$payload.out.push(`<h6${attr_class(clsx(node.className || ""))}>`);
                    MarkdownRenderer($$payload, { nodes: node.children || [] });
                    $$payload.out.push(`<!----></h6>`);
                  } else {
                    $$payload.out.push("<!--[!-->");
                  }
                  $$payload.out.push(`<!--]-->`);
                }
                $$payload.out.push(`<!--]-->`);
              }
              $$payload.out.push(`<!--]-->`);
            }
            $$payload.out.push(`<!--]-->`);
          }
          $$payload.out.push(`<!--]-->`);
        }
        $$payload.out.push(`<!--]-->`);
      } else {
        $$payload.out.push("<!--[!-->");
        if (node.type === "bold") {
          $$payload.out.push("<!--[-->");
          $$payload.out.push(`<strong${attr_class(clsx(node.className || ""))}>`);
          MarkdownRenderer($$payload, { nodes: node.children || [] });
          $$payload.out.push(`<!----></strong>`);
        } else {
          $$payload.out.push("<!--[!-->");
          if (node.type === "italic") {
            $$payload.out.push("<!--[-->");
            $$payload.out.push(`<em${attr_class(clsx(node.className || ""))}>`);
            MarkdownRenderer($$payload, { nodes: node.children || [] });
            $$payload.out.push(`<!----></em>`);
          } else {
            $$payload.out.push("<!--[!-->");
            if (node.type === "code") {
              $$payload.out.push("<!--[-->");
              $$payload.out.push(`<code${attr_class(clsx(node.className || ""))}>${escape_html(node.content || "")}</code>`);
            } else {
              $$payload.out.push("<!--[!-->");
              if (node.type === "link") {
                $$payload.out.push("<!--[-->");
                $$payload.out.push(`<a${attr("href", node.href || "")}${attr_class(clsx(node.className || ""))} target="_blank" rel="noopener noreferrer">`);
                MarkdownRenderer($$payload, { nodes: node.children || [] });
                $$payload.out.push(`<!----></a>`);
              } else {
                $$payload.out.push("<!--[!-->");
                if (node.type === "br") {
                  $$payload.out.push("<!--[-->");
                  $$payload.out.push(`<br/>`);
                } else {
                  $$payload.out.push("<!--[!-->");
                  if (node.type === "text") {
                    $$payload.out.push("<!--[-->");
                    $$payload.out.push(`${escape_html(node.content || "")}`);
                  } else {
                    $$payload.out.push("<!--[!-->");
                  }
                  $$payload.out.push(`<!--]-->`);
                }
                $$payload.out.push(`<!--]-->`);
              }
              $$payload.out.push(`<!--]-->`);
            }
            $$payload.out.push(`<!--]-->`);
          }
          $$payload.out.push(`<!--]-->`);
        }
        $$payload.out.push(`<!--]-->`);
      }
      $$payload.out.push(`<!--]-->`);
    }
    $$payload.out.push(`<!--]-->`);
  }
  $$payload.out.push(`<!--]-->`);
  bind_props($$props, { content, nodes });
  pop();
}
class MockTextService {
  static instance;
  textNodes = /* @__PURE__ */ new Map();
  autoSaveTimeouts = /* @__PURE__ */ new Map();
  static getInstance() {
    if (!MockTextService.instance) {
      MockTextService.instance = new MockTextService();
    }
    return MockTextService.instance;
  }
  constructor() {
    this.initializeSampleData();
  }
  initializeSampleData() {
    const rootNode = {
      id: "text-root-1",
      content: "This is the **root node** of our hierarchical structure.\n\nIt contains several child nodes demonstrating tree patterns.",
      title: "Project Root",
      parentId: null,
      depth: 0,
      expanded: true,
      createdAt: new Date(Date.now() - 864e5),
      updatedAt: new Date(Date.now() - 36e5),
      metadata: {
        wordCount: 18,
        lastEditedBy: "user",
        version: 1,
        hasChildren: true,
        childrenIds: ["text-child-1", "text-child-2"]
      }
    };
    const childNode1 = {
      id: "text-child-1",
      content: "This is the **first child node**.\n\nIt has its own children to demonstrate multi-level hierarchy.",
      title: "Chapter 1: Introduction",
      parentId: "text-root-1",
      depth: 1,
      expanded: true,
      createdAt: new Date(Date.now() - 828e5),
      updatedAt: new Date(Date.now() - 72e5),
      metadata: {
        wordCount: 16,
        lastEditedBy: "user",
        version: 2,
        hasChildren: true,
        childrenIds: ["text-grandchild-1", "text-grandchild-2"]
      }
    };
    const childNode2 = {
      id: "text-child-2",
      content: "This is the **second child node**.\n\nIt's a leaf node with no children.",
      title: "Chapter 2: Conclusion",
      parentId: "text-root-1",
      depth: 1,
      expanded: false,
      createdAt: new Date(Date.now() - 792e5),
      updatedAt: new Date(Date.now() - 36e5),
      metadata: {
        wordCount: 14,
        lastEditedBy: "user",
        version: 1,
        hasChildren: false,
        childrenIds: []
      }
    };
    const grandchildNode1 = {
      id: "text-grandchild-1",
      content: "This is a **grandchild node** at depth 2.\n\n- Demonstrates deeper hierarchy\n- Shows indentation patterns",
      title: "Section 1.1: Core Concepts",
      parentId: "text-child-1",
      depth: 2,
      expanded: false,
      createdAt: new Date(Date.now() - 756e5),
      updatedAt: new Date(Date.now() - 18e5),
      metadata: {
        wordCount: 15,
        lastEditedBy: "user",
        version: 1,
        hasChildren: false,
        childrenIds: []
      }
    };
    const grandchildNode2 = {
      id: "text-grandchild-2",
      content: "Another **grandchild node** showing sibling relationships.\n\nThis completes our hierarchy example.",
      title: "Section 1.2: Advanced Features",
      parentId: "text-child-1",
      depth: 2,
      expanded: false,
      createdAt: new Date(Date.now() - 72e6),
      updatedAt: new Date(Date.now() - 9e5),
      metadata: {
        wordCount: 13,
        lastEditedBy: "user",
        version: 1,
        hasChildren: false,
        childrenIds: []
      }
    };
    this.textNodes.set(rootNode.id, rootNode);
    this.textNodes.set(childNode1.id, childNode1);
    this.textNodes.set(childNode2.id, childNode2);
    this.textNodes.set(grandchildNode1.id, grandchildNode1);
    this.textNodes.set(grandchildNode2.id, grandchildNode2);
  }
  /**
   * Save text node content with auto-save simulation
   */
  async saveTextNode(id, content, title) {
    await new Promise((resolve) => setTimeout(resolve, 200 + Math.random() * 300));
    try {
      const now = /* @__PURE__ */ new Date();
      const existingNode = this.textNodes.get(id);
      const nodeData = {
        id,
        content,
        title: title || existingNode?.title || "Untitled",
        parentId: existingNode?.parentId || null,
        depth: existingNode?.depth || 0,
        expanded: existingNode?.expanded ?? true,
        createdAt: existingNode?.createdAt || now,
        updatedAt: now,
        metadata: {
          wordCount: this.calculateWordCount(content),
          lastEditedBy: "user",
          version: (existingNode?.metadata.version || 0) + 1,
          hasChildren: existingNode?.metadata.hasChildren || false,
          childrenIds: existingNode?.metadata.childrenIds || []
        }
      };
      this.textNodes.set(id, nodeData);
      return {
        success: true,
        id,
        timestamp: now
      };
    } catch (error) {
      return {
        success: false,
        id,
        timestamp: /* @__PURE__ */ new Date(),
        error: error instanceof Error ? error.message : "Unknown error"
      };
    }
  }
  /**
   * Load text node by ID
   */
  async loadTextNode(id) {
    await new Promise((resolve) => setTimeout(resolve, 100 + Math.random() * 200));
    return this.textNodes.get(id) || null;
  }
  /**
   * Create new text node
   */
  async createTextNode(content = "", title = "New Text Node") {
    await new Promise((resolve) => setTimeout(resolve, 150));
    const id = this.generateId();
    const now = /* @__PURE__ */ new Date();
    const nodeData = {
      id,
      content,
      title,
      parentId: null,
      depth: 0,
      expanded: true,
      createdAt: now,
      updatedAt: now,
      metadata: {
        wordCount: this.calculateWordCount(content),
        lastEditedBy: "user",
        version: 1,
        hasChildren: false,
        childrenIds: []
      }
    };
    this.textNodes.set(id, nodeData);
    return nodeData;
  }
  /**
   * Delete text node
   */
  async deleteTextNode(id) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    const timeout = this.autoSaveTimeouts.get(id);
    if (timeout) {
      clearTimeout(timeout);
      this.autoSaveTimeouts.delete(id);
    }
    return this.textNodes.delete(id);
  }
  /**
   * Auto-save with debouncing
   */
  scheduleAutoSave(id, content, title, delay = 2e3) {
    return new Promise((resolve) => {
      const existingTimeout = this.autoSaveTimeouts.get(id);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }
      const timeout = setTimeout(async () => {
        const result = await this.saveTextNode(id, content, title);
        this.autoSaveTimeouts.delete(id);
        resolve(result);
      }, delay);
      this.autoSaveTimeouts.set(id, timeout);
    });
  }
  /**
   * Cancel pending auto-save for a node
   */
  cancelAutoSave(id) {
    const timeout = this.autoSaveTimeouts.get(id);
    if (timeout) {
      clearTimeout(timeout);
      this.autoSaveTimeouts.delete(id);
    }
  }
  /**
   * Get all text nodes (for listing/search)
   */
  async getAllTextNodes() {
    await new Promise((resolve) => setTimeout(resolve, 150));
    return Array.from(this.textNodes.values()).sort(
      (a, b) => b.updatedAt.getTime() - a.updatedAt.getTime()
    );
  }
  /**
   * Search text nodes by content or title
   */
  async searchTextNodes(query) {
    await new Promise((resolve) => setTimeout(resolve, 200));
    if (!query.trim()) {
      return this.getAllTextNodes();
    }
    const searchLower = query.toLowerCase();
    return Array.from(this.textNodes.values()).filter(
      (node) => node.title.toLowerCase().includes(searchLower) || node.content.toLowerCase().includes(searchLower)
    ).sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
  }
  /**
   * Generate unique node ID
   */
  generateId() {
    return `text-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
  }
  /**
   * Calculate word count for content
   */
  calculateWordCount(content) {
    return content.trim().split(/\s+/).filter((word) => word.length > 0).length;
  }
  /**
   * Get hierarchical tree structure for UI rendering
   */
  async getHierarchicalNodes() {
    await new Promise((resolve) => setTimeout(resolve, 100));
    const allNodes = Array.from(this.textNodes.values());
    const nodeMap = /* @__PURE__ */ new Map();
    for (const node of allNodes) {
      nodeMap.set(node.id, {
        id: node.id,
        title: node.title,
        content: node.content,
        nodeType: "text",
        depth: node.depth,
        parentId: node.parentId,
        children: [],
        expanded: node.expanded,
        hasChildren: node.metadata.hasChildren,
        createdAt: node.createdAt,
        updatedAt: node.updatedAt,
        metadata: {
          wordCount: node.metadata.wordCount,
          lastEditedBy: node.metadata.lastEditedBy,
          version: node.metadata.version
        }
      });
    }
    const rootNodes = [];
    for (const hierarchicalNode of nodeMap.values()) {
      if (hierarchicalNode.parentId === null) {
        rootNodes.push(hierarchicalNode);
      } else {
        const parent = nodeMap.get(hierarchicalNode.parentId);
        if (parent) {
          parent.children.push(hierarchicalNode);
        }
      }
    }
    function sortChildren(nodes) {
      nodes.sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime());
      nodes.forEach((node) => sortChildren(node.children));
    }
    sortChildren(rootNodes);
    return rootNodes;
  }
  /**
   * Create a child node under a parent
   */
  async createChildNode(parentId, content = "", title = "New Child Node") {
    await new Promise((resolve) => setTimeout(resolve, 150));
    const parent = this.textNodes.get(parentId);
    if (!parent) return null;
    const id = this.generateId();
    const now = /* @__PURE__ */ new Date();
    const childNode = {
      id,
      content,
      title,
      parentId,
      depth: parent.depth + 1,
      expanded: false,
      createdAt: now,
      updatedAt: now,
      metadata: {
        wordCount: this.calculateWordCount(content),
        lastEditedBy: "user",
        version: 1,
        hasChildren: false,
        childrenIds: []
      }
    };
    parent.metadata.hasChildren = true;
    parent.metadata.childrenIds.push(id);
    parent.updatedAt = now;
    this.textNodes.set(id, childNode);
    this.textNodes.set(parentId, parent);
    return childNode;
  }
  /**
   * Move a node to a different parent (or make it root-level)
   */
  async moveNode(nodeId, newParentId) {
    await new Promise((resolve) => setTimeout(resolve, 200));
    const node = this.textNodes.get(nodeId);
    if (!node) return false;
    const oldParentId = node.parentId;
    if (oldParentId) {
      const oldParent = this.textNodes.get(oldParentId);
      if (oldParent) {
        oldParent.metadata.childrenIds = oldParent.metadata.childrenIds.filter(
          (id) => id !== nodeId
        );
        oldParent.metadata.hasChildren = oldParent.metadata.childrenIds.length > 0;
        oldParent.updatedAt = /* @__PURE__ */ new Date();
        this.textNodes.set(oldParentId, oldParent);
      }
    }
    if (newParentId) {
      const newParent = this.textNodes.get(newParentId);
      if (!newParent) return false;
      node.parentId = newParentId;
      node.depth = newParent.depth + 1;
      newParent.metadata.hasChildren = true;
      newParent.metadata.childrenIds.push(nodeId);
      newParent.updatedAt = /* @__PURE__ */ new Date();
      this.textNodes.set(newParentId, newParent);
    } else {
      node.parentId = null;
      node.depth = 0;
    }
    this.updateDescendantDepths(nodeId, node.depth);
    node.updatedAt = /* @__PURE__ */ new Date();
    this.textNodes.set(nodeId, node);
    return true;
  }
  /**
   * Toggle expansion state of a node
   */
  async toggleNodeExpansion(nodeId) {
    await new Promise((resolve) => setTimeout(resolve, 50));
    const node = this.textNodes.get(nodeId);
    if (!node || !node.metadata.hasChildren) return false;
    node.expanded = !node.expanded;
    node.updatedAt = /* @__PURE__ */ new Date();
    this.textNodes.set(nodeId, node);
    return node.expanded;
  }
  /**
   * Get children of a specific node
   */
  async getChildren(parentId) {
    await new Promise((resolve) => setTimeout(resolve, 75));
    const parent = this.textNodes.get(parentId);
    if (!parent) return [];
    return parent.metadata.childrenIds.map((id) => this.textNodes.get(id)).filter((node) => node !== void 0).sort((a, b) => a.createdAt.getTime() - b.createdAt.getTime());
  }
  /**
   * Update depths of all descendant nodes recursively
   */
  updateDescendantDepths(nodeId, newDepth) {
    const node = this.textNodes.get(nodeId);
    if (!node) return;
    node.depth = newDepth;
    for (const childId of node.metadata.childrenIds) {
      this.updateDescendantDepths(childId, newDepth + 1);
    }
  }
  /**
   * Get service statistics (for debugging/monitoring)
   */
  getStats() {
    const nodes = Array.from(this.textNodes.values());
    const rootNodes = nodes.filter((node) => node.parentId === null);
    const maxDepth = Math.max(...nodes.map((node) => node.depth), 0);
    return {
      totalNodes: this.textNodes.size,
      rootNodes: rootNodes.length,
      maxDepth,
      nodesWithChildren: nodes.filter((node) => node.metadata.hasChildren).length,
      pendingAutoSaves: this.autoSaveTimeouts.size,
      lastActivity: /* @__PURE__ */ new Date()
    };
  }
}
MockTextService.getInstance();
function TextNode($$payload, $$props) {
  push();
  let isMultiline;
  let nodeId = $$props["nodeId"];
  let content = fallback($$props["content"], "");
  let editable = fallback($$props["editable"], true);
  let markdown = fallback($$props["markdown"], true);
  let placeholder = fallback($$props["placeholder"], "Click to add text...");
  let autoSave = fallback($$props["autoSave"], true);
  let compact = fallback($$props["compact"], false);
  let saveStatus = "saved";
  isMultiline = content.includes("\n");
  let $$settled = true;
  let $$inner_payload;
  function $$render_inner($$payload2) {
    BaseNode($$payload2, {
      nodeId,
      nodeType: "text",
      hasChildren: false,
      isProcessing: saveStatus === "saving",
      editable,
      contentEditable: true,
      multiline: isMultiline,
      placeholder,
      className: `ns-text-node ${stringify(compact ? "ns-text-node--compact" : "")}`,
      get content() {
        return content;
      },
      set content($$value) {
        content = $$value;
        $$settled = false;
      },
      children: ($$payload3) => {
        {
          $$payload3.out.push("<!--[!-->");
          {
            $$payload3.out.push("<!--[!-->");
          }
          $$payload3.out.push(`<!--]-->`);
        }
        $$payload3.out.push(`<!--]-->`);
      },
      $$slots: {
        default: true,
        "display-content": ($$payload3) => {
          $$payload3.out.push(`<div slot="display-content">`);
          if (content) {
            $$payload3.out.push("<!--[-->");
            if (markdown) {
              $$payload3.out.push("<!--[-->");
              $$payload3.out.push(`<div class="ns-text-node__markdown svelte-7k70ck">`);
              MarkdownRenderer($$payload3, { content });
              $$payload3.out.push(`<!----></div>`);
            } else {
              $$payload3.out.push("<!--[!-->");
              $$payload3.out.push(`<span class="ns-node__text">${escape_html(content)}</span>`);
            }
            $$payload3.out.push(`<!--]-->`);
          } else {
            $$payload3.out.push("<!--[!-->");
            $$payload3.out.push(`<span class="ns-node__empty">${escape_html(placeholder)}</span>`);
          }
          $$payload3.out.push(`<!--]--></div>`);
        }
      }
    });
  }
  do {
    $$settled = true;
    $$inner_payload = copy_payload($$payload);
    $$render_inner($$inner_payload);
  } while (!$$settled);
  assign_payload($$payload, $$inner_payload);
  bind_props($$props, {
    nodeId,
    content,
    editable,
    markdown,
    placeholder,
    autoSave,
    compact
  });
  pop();
}
function NodeTree($$payload, $$props) {
  push();
  let visibleNodes;
  let nodes = fallback($$props["nodes"], () => [], true);
  const maxDepth = 10;
  let indentSize = fallback($$props["indentSize"], 4);
  const expandedByDefault = true;
  let showExpandControls = fallback($$props["showExpandControls"], true);
  let allowEdit = fallback($$props["allowEdit"], true);
  function getVisibleNodes(nodeList) {
    const result = [];
    function addVisibleChildren(nodes2) {
      for (const node of nodes2) {
        result.push(node);
        if (node.expanded && node.children.length > 0) {
          addVisibleChildren(node.children);
        }
      }
    }
    const rootNodes = nodeList.filter((node) => node.depth === 0);
    addVisibleChildren(rootNodes);
    return result;
  }
  function getExpandIcon(node) {
    if (!node.hasChildren) return "";
    return node.expanded ? "▼" : "▶";
  }
  visibleNodes = getVisibleNodes(nodes);
  const each_array = ensure_array_like(visibleNodes);
  $$payload.out.push(`<div class="ns-node-tree svelte-cbhleu" role="tree" aria-label="Node hierarchy"><!--[-->`);
  for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
    let node = each_array[$$index];
    $$payload.out.push(`<div class="ns-node-tree__item svelte-cbhleu"${attr_style(`margin-left: calc(var(--ns-spacing-${stringify(indentSize)}) * ${stringify(node.depth)})`)} role="treeitem"${attr("aria-level", node.depth + 1)}${attr("aria-expanded", node.hasChildren ? node.expanded : void 0)} aria-selected="false">`);
    if (showExpandControls && node.hasChildren) {
      $$payload.out.push("<!--[-->");
      $$payload.out.push(`<button type="button" class="ns-node-tree__expand-btn svelte-cbhleu"${attr("aria-label", `${stringify(node.expanded ? "Collapse" : "Expand")} ${stringify(node.title || "Untitled node")}`)}><span class="ns-node-tree__expand-icon svelte-cbhleu">${escape_html(getExpandIcon(node))}</span></button>`);
    } else {
      $$payload.out.push("<!--[!-->");
      $$payload.out.push(`<div class="ns-node-tree__expand-spacer svelte-cbhleu"></div>`);
    }
    $$payload.out.push(`<!--]--> <div class="ns-node-tree__content svelte-cbhleu">`);
    if (node.nodeType === "text") {
      $$payload.out.push("<!--[-->");
      TextNode($$payload, { nodeId: node.id, content: node.content, editable: allowEdit });
    } else {
      $$payload.out.push("<!--[!-->");
      $$payload.out.push(`<div class="ns-node-tree__placeholder svelte-cbhleu"><p class="svelte-cbhleu">Node type '${escape_html(node.nodeType)}' not yet implemented</p> <p class="svelte-cbhleu">ID: ${escape_html(node.id)} | Depth: ${escape_html(node.depth)}</p></div>`);
    }
    $$payload.out.push(`<!--]--></div></div>`);
  }
  $$payload.out.push(`<!--]--></div>`);
  bind_props($$props, {
    nodes,
    indentSize,
    showExpandControls,
    allowEdit,
    maxDepth,
    expandedByDefault
  });
  pop();
}
function HierarchyDemo($$payload, $$props) {
  push();
  let hierarchicalNodes = [];
  function countDescendants(node) {
    return node.children.length + node.children.reduce((sum, child) => sum + countDescendants(child), 0);
  }
  function getAllNodes(nodes) {
    const result = [];
    function collect(nodeList) {
      for (const node of nodeList) {
        result.push(node);
        collect(node.children);
      }
    }
    collect(nodes);
    return result;
  }
  ({
    totalNodes: hierarchicalNodes.length + hierarchicalNodes.reduce((sum, node) => sum + countDescendants(node), 0),
    rootNodes: hierarchicalNodes.filter((node) => node.depth === 0).length,
    maxDepth: Math.max(...getAllNodes(hierarchicalNodes).map((node) => node.depth), 0)
  });
  $$payload.out.push(`<div class="hierarchy-demo svelte-r4tqc6"><header class="hierarchy-demo__header svelte-r4tqc6"><h2 class="hierarchy-demo__title svelte-r4tqc6">Hierarchical Display Patterns</h2> <p class="hierarchy-demo__description svelte-r4tqc6">Demonstrates tree indentation, expand/collapse functionality, and parent-child node
      relationships.</p> `);
  {
    $$payload.out.push("<!--[!-->");
  }
  $$payload.out.push(`<!--]--></header> <main class="hierarchy-demo__content svelte-r4tqc6">`);
  {
    $$payload.out.push("<!--[-->");
    $$payload.out.push(`<div class="hierarchy-demo__loading svelte-r4tqc6"><div class="spinner svelte-r4tqc6"></div> <p>Loading hierarchical structure...</p></div>`);
  }
  $$payload.out.push(`<!--]--></main> <aside class="hierarchy-demo__info svelte-r4tqc6"><h3 class="svelte-r4tqc6">Features Demonstrated:</h3> <ul class="svelte-r4tqc6"><li class="svelte-r4tqc6"><strong>Tree Indentation:</strong> CSS <code class="svelte-r4tqc6">margin-left: calc(var(--ns-spacing-4) * depth)</code></li> <li class="svelte-r4tqc6"><strong>Parent Indicators:</strong> Ring effects when nodes have children</li> <li class="svelte-r4tqc6"><strong>Expand/Collapse:</strong> Toggle visibility of child nodes</li> <li class="svelte-r4tqc6"><strong>Depth Tracking:</strong> Automatic depth calculation for proper nesting</li> <li class="svelte-r4tqc6"><strong>Interactive Editing:</strong> Click-to-edit functionality on all nodes</li> <li class="svelte-r4tqc6"><strong>Hierarchy Management:</strong> Parent-child relationship tracking</li></ul></aside></div>`);
  pop();
}
function TextNodeDemo($$payload, $$props) {
  push();
  let demoNodes = [
    {
      id: "demo-1",
      content: "Simple text without markdown. Click to edit!",
      editable: true,
      markdown: false
    },
    {
      id: "demo-2",
      content: "# Welcome to NodeSpace\n\nThis TextNode supports **markdown** formatting:\n\n- *Italic text*\n- **Bold text** \n- `inline code`\n- [Links](https://example.com)\n\n## Features\n\n1. Multi-line editing\n2. Auto-save\n3. Markdown rendering",
      editable: true,
      markdown: true
    },
    {
      id: "demo-3",
      content: "This node is **read-only** and cannot be edited. It still renders markdown but demonstrates display-only mode.",
      editable: false,
      markdown: true
    },
    { id: "new-node", content: "", editable: true, markdown: true }
  ];
  const each_array = ensure_array_like(demoNodes);
  $$payload.out.push(`<div class="text-node-demo svelte-ocjckk"><h1 class="demo-title svelte-ocjckk">TextNode Component Demo</h1> <div class="demo-description svelte-ocjckk"><p class="svelte-ocjckk">This demo showcases the TextNode component with various configurations:</p> <ul class="svelte-ocjckk"><li class="svelte-ocjckk"><strong>Click-to-edit:</strong> Click on any editable node to start editing</li> <li class="svelte-ocjckk"><strong>Keyboard shortcuts:</strong> Ctrl+Enter to save, Esc to cancel</li> <li class="svelte-ocjckk"><strong>Auto-save:</strong> Changes are automatically saved after 2 seconds</li> <li class="svelte-ocjckk"><strong>Markdown:</strong> Support for basic markdown formatting</li> <li class="svelte-ocjckk"><strong>Auto-resize:</strong> Textarea expands to fit content</li></ul></div> <div class="demo-nodes svelte-ocjckk"><!--[-->`);
  for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
    let node = each_array[$$index];
    $$payload.out.push(`<div class="demo-node-container svelte-ocjckk"><h3 class="demo-node-title svelte-ocjckk">${escape_html(node.id === "new-node" ? "New Node" : `Demo Node ${node.id.split("-")[1]}`)} <span class="demo-node-config svelte-ocjckk">${escape_html(node.editable ? "(Editable)" : "(Read-only)")}
            ${escape_html(node.markdown ? " + Markdown" : "")}</span></h3> `);
    TextNode($$payload, {
      nodeId: node.id,
      content: node.content,
      editable: node.editable,
      markdown: node.markdown,
      placeholder: node.id === "new-node" ? "Start typing to create a new text node..." : "Click to add text..."
    });
    $$payload.out.push(`<!----></div>`);
  }
  $$payload.out.push(`<!--]--></div> <div class="demo-instructions svelte-ocjckk"><h2 class="svelte-ocjckk">Try These Features:</h2> <div class="instruction-grid svelte-ocjckk"><div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Basic Editing</h4> <p class="svelte-ocjckk">Click on the first node to start editing. Type some text and press Ctrl+Enter to save, or
          Esc to cancel.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Markdown Formatting</h4> <p class="svelte-ocjckk">Try the second node with markdown. Add **bold text**, *italic text*, # headers, and see
          them rendered.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Multi-line Editing</h4> <p class="svelte-ocjckk">TextNodes support multi-line editing. Use Shift+Enter for line breaks, or just press Enter
          normally in the textarea.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Markdown Rendering</h4> <p class="svelte-ocjckk">Second node shows markdown rendering in display mode. Edit to see raw markdown, blur to
          see rendered output.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Read-only Mode</h4> <p class="svelte-ocjckk">Third node demonstrates read-only display with markdown rendering but no editing
          capability.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Auto-resize</h4> <p class="svelte-ocjckk">Add multiple lines of text to see the textarea automatically expand.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">New Node</h4> <p class="svelte-ocjckk">The last node starts empty. Click to add content and it will be automatically saved.</p></div> <div class="instruction svelte-ocjckk"><h4 class="svelte-ocjckk">Read-only</h4> <p class="svelte-ocjckk">The third node cannot be edited, demonstrating display-only mode.</p></div></div></div></div>`);
  pop();
}
const DEFAULT_CONFIG = {
  createNewNodes: true,
  preserveBulletSyntax: false,
  maxDepth: 10,
  defaultNodeType: "text",
  indentSize: 4
};
class BulletToNodeConverter {
  config;
  constructor(config = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }
  /**
   * Convert bullet patterns in content to node hierarchy
   */
  convertBulletsToNodes(content, patterns, cursorPosition, parentNodeId) {
    const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(patterns);
    if (bulletPatterns.length === 0) {
      return {
        hasConversions: false,
        cleanedContent: content,
        newNodes: [],
        newCursorPosition: cursorPosition
      };
    }
    const bulletMatches = this.analyzeBulletStructure(content, bulletPatterns);
    const nodeHierarchy = this.buildNodeHierarchy(bulletMatches, parentNodeId);
    const cleanedContent = this.removeBulletSyntax(content, bulletPatterns);
    const newCursorPosition = this.calculateNewCursorPosition(
      content,
      cursorPosition,
      bulletPatterns
    );
    return {
      hasConversions: true,
      cleanedContent,
      newNodes: nodeHierarchy,
      newCursorPosition,
      parentNodeId
    };
  }
  /**
   * Detect bullet typing in real-time and trigger conversion
   */
  detectBulletTyping(content, patterns, cursorPosition) {
    const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(patterns);
    for (const pattern of bulletPatterns) {
      const bulletEndPosition = pattern.start + pattern.syntax.length;
      if (cursorPosition >= bulletEndPosition && cursorPosition <= bulletEndPosition + 2) {
        return true;
      }
    }
    const currentLine = this.getCurrentLine(content, cursorPosition);
    const bulletRegex = /^(\s*)([-*+])\s(.*)$/;
    const match = currentLine.trim().match(bulletRegex);
    return match !== null;
  }
  /**
   * Handle undo/correction scenarios
   */
  undoBulletConversion(originalContent, convertedNodes) {
    let restoredContent = originalContent;
    for (const node of convertedNodes) {
      const bulletSyntax = this.getBulletSyntaxForDepth(node.depth);
      const bulletLine = `${bulletSyntax}${node.content}`;
      restoredContent += `
${bulletLine}`;
    }
    return restoredContent;
  }
  /**
   * Private helper methods
   */
  analyzeBulletStructure(content, patterns) {
    const lines = content.split("\n");
    const matches = [];
    for (const pattern of patterns) {
      const lineNumber = pattern.line;
      const lineContent = lines[lineNumber] || "";
      const leadingWhitespace = lineContent.match(/^(\s*)/)?.[1] || "";
      const indentLevel = Math.floor(leadingWhitespace.length / 2);
      const cleanContent = pattern.content.trim();
      matches.push({
        pattern,
        indentLevel: Math.min(indentLevel, this.config.maxDepth),
        cleanContent,
        lineNumber
      });
    }
    return matches.sort((a, b) => a.lineNumber - b.lineNumber);
  }
  buildNodeHierarchy(matches, parentNodeId) {
    const nodes = [];
    const nodeStack = [];
    for (const match of matches) {
      const nodeId = this.generateNodeId();
      let actualParentId = parentNodeId;
      let targetParent;
      if (match.indentLevel > 0 && nodeStack.length > 0) {
        const parentLevel = match.indentLevel - 1;
        targetParent = nodeStack[parentLevel];
        if (targetParent) {
          actualParentId = targetParent.id;
        }
      }
      const newNode = this.createNodeData(nodeId, match, actualParentId);
      if (targetParent) {
        targetParent.hasChildren = true;
        targetParent.children.push(newNode);
      } else {
        nodes.push(newNode);
      }
      nodeStack[match.indentLevel] = newNode;
      nodeStack.splice(match.indentLevel + 1);
    }
    return nodes;
  }
  createNodeData(nodeId, match, parentId) {
    return {
      id: nodeId,
      title: match.cleanContent.substring(0, 50),
      // First 50 chars as title
      content: match.cleanContent,
      nodeType: this.config.defaultNodeType,
      depth: match.indentLevel,
      parentId: parentId || null,
      children: [],
      expanded: true,
      hasChildren: false
    };
  }
  removeBulletSyntax(content, patterns) {
    let cleanContent = content;
    const sortedPatterns = [...patterns].sort((a, b) => b.start - a.start);
    for (const pattern of sortedPatterns) {
      const lines = cleanContent.split("\n");
      const line = lines[pattern.line];
      if (line) {
        const bulletRegex = new RegExp(`^(\\s*)([-*+])\\s`);
        const cleanedLine = line.replace(bulletRegex, "$1");
        lines[pattern.line] = cleanedLine;
        cleanContent = lines.join("\n");
      }
    }
    return cleanContent;
  }
  calculateNewCursorPosition(originalContent, originalPosition, patterns) {
    let newPosition = originalPosition;
    for (const pattern of patterns) {
      if (pattern.start < originalPosition) {
        newPosition -= pattern.syntax.length;
      }
    }
    return Math.max(0, newPosition);
  }
  getCurrentLine(content, position) {
    const lines = content.split("\n");
    let currentPos = 0;
    for (const line of lines) {
      if (position <= currentPos + line.length) {
        return line;
      }
      currentPos += line.length + 1;
    }
    return lines[lines.length - 1] || "";
  }
  getBulletSyntaxForDepth(depth) {
    const bulletTypes = ["- ", "* ", "+ "];
    const bulletType = bulletTypes[depth % bulletTypes.length];
    const indent = " ".repeat(depth * this.config.indentSize);
    return indent + bulletType;
  }
  generateNodeId() {
    return `node-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}
new BulletToNodeConverter();
new BulletToNodeConverter({
  defaultNodeType: "task",
  createNewNodes: true,
  preserveBulletSyntax: false
});
function BulletConversionDemo($$payload, $$props) {
  push();
  let totalNodes, bulletNodes;
  let nodes = [];
  let showDebugInfo = false;
  let conversionCount = 0;
  totalNodes = nodes.length;
  bulletNodes = nodes.filter((node) => node.parentId !== null).length;
  nodes.filter((node) => node.parentId === null).length;
  let $$settled = true;
  let $$inner_payload;
  function $$render_inner($$payload2) {
    $$payload2.out.push(`<div class="bullet-demo svelte-1pq12kh"><header class="bullet-demo__header svelte-1pq12kh"><h2 class="svelte-1pq12kh">Smart Bullet-to-Node Conversion Demo</h2> <p class="svelte-1pq12kh">Type bullet points using <code class="svelte-1pq12kh">-</code>, <code class="svelte-1pq12kh">*</code>, or <code class="svelte-1pq12kh">+</code> to see them automatically convert to child nodes!</p></header> <div class="bullet-demo__controls svelte-1pq12kh"><button type="button" class="demo-button svelte-1pq12kh">Load Sample Content</button> <button type="button" class="demo-button demo-button--secondary svelte-1pq12kh">Clear All</button> <label class="demo-checkbox svelte-1pq12kh"><input type="checkbox"${attr("checked", showDebugInfo, true)}/> Show Debug Info</label></div> <div class="bullet-demo__stats svelte-1pq12kh"><div class="stats-grid svelte-1pq12kh"><div class="stat-item svelte-1pq12kh"><span class="stat-label svelte-1pq12kh">Total Nodes:</span> <span class="stat-value svelte-1pq12kh">${escape_html(totalNodes)}</span></div> <div class="stat-item svelte-1pq12kh"><span class="stat-label svelte-1pq12kh">Child Nodes:</span> <span class="stat-value svelte-1pq12kh">${escape_html(bulletNodes)}</span></div> <div class="stat-item svelte-1pq12kh"><span class="stat-label svelte-1pq12kh">Conversions:</span> <span class="stat-value svelte-1pq12kh">${escape_html(conversionCount)}</span></div> `);
    {
      $$payload2.out.push("<!--[!-->");
    }
    $$payload2.out.push(`<!--]--></div></div> <div class="bullet-demo__content svelte-1pq12kh"><div class="bullet-demo__editor svelte-1pq12kh"><h3 class="svelte-1pq12kh">Smart Text Editor</h3> <div class="editor-container svelte-1pq12kh">`);
    {
      $$payload2.out.push("<!--[!-->");
      $$payload2.out.push(`<p class="no-selection svelte-1pq12kh">Select a node from the tree to edit it</p>`);
    }
    $$payload2.out.push(`<!--]--></div></div> <div class="bullet-demo__tree svelte-1pq12kh"><h3 class="svelte-1pq12kh">Node Hierarchy</h3> <div class="tree-container svelte-1pq12kh">`);
    NodeTree($$payload2, { nodes, allowEdit: false, showExpandControls: true });
    $$payload2.out.push(`<!----></div></div></div> `);
    {
      $$payload2.out.push("<!--[!-->");
    }
    $$payload2.out.push(`<!--]--></div>`);
  }
  do {
    $$settled = true;
    $$inner_payload = copy_payload($$payload);
    $$render_inner($$inner_payload);
  } while (!$$settled);
  assign_payload($$payload, $$inner_payload);
  pop();
}
function SoftNewlineDemo($$payload, $$props) {
  push();
  let demoNodes = [
    {
      id: "demo-1",
      content: "Try typing some text here, then press Shift-Enter...",
      type: "text"
    }
  ];
  let softNewlineContexts = {};
  let suggestions = [];
  let lastActivityLog = [];
  function formatPatternType(type) {
    const typeMap = {
      "header": "📋 Header",
      "bullet": "• Bullet",
      "blockquote": "💬 Quote",
      "codeblock": "💻 Code",
      "bold": "**Bold**",
      "italic": "*Italic*",
      "inlinecode": "`Code`"
    };
    return typeMap[type] || type;
  }
  function formatNodeType(type) {
    const typeMap = {
      "text": "📝 Text",
      "task": "✅ Task",
      "ai-chat": "🤖 AI Chat",
      "entity": "👤 Entity",
      "query": "🔍 Query"
    };
    return typeMap[type] || type;
  }
  const each_array = ensure_array_like(demoNodes);
  const each_array_1 = ensure_array_like(Object.entries(softNewlineContexts));
  const each_array_2 = ensure_array_like(suggestions);
  const each_array_3 = ensure_array_like(lastActivityLog);
  $$payload.out.push(`<div class="soft-newline-demo svelte-pnpw1v"><div class="demo-header svelte-pnpw1v"><h2 class="svelte-pnpw1v">Soft Newline + Markdown Detection Demo</h2> <p class="demo-description svelte-pnpw1v">Try this: Type some text, press <kbd class="svelte-pnpw1v">Shift+Enter</kbd> to create a soft newline, 
      then type markdown syntax like <code class="svelte-pnpw1v"># Header</code>, <code class="svelte-pnpw1v">- Bullet</code>, 
      or <code class="svelte-pnpw1v">> Quote</code> to see automatic node suggestions.</p></div> <div class="demo-container svelte-pnpw1v"><div class="demo-nodes svelte-pnpw1v"><h3 class="svelte-pnpw1v">Demo Nodes</h3> <!--[-->`);
  for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
    let node = each_array[$$index];
    $$payload.out.push(`<div class="demo-node-wrapper svelte-pnpw1v"><div class="node-type-badge svelte-pnpw1v">${escape_html(formatNodeType(node.type))}</div> `);
    BaseNode($$payload, {
      nodeId: node.id,
      nodeType: node.type,
      content: node.content,
      multiline: true,
      editable: true,
      contentEditable: true,
      placeholder: "Start typing... (try Shift-Enter then markdown syntax)"
    });
    $$payload.out.push(`<!----></div>`);
  }
  $$payload.out.push(`<!--]--> <button class="add-node-btn svelte-pnpw1v">+ Add Demo Node</button></div> <div class="demo-sidebar svelte-pnpw1v"><div class="context-panel svelte-pnpw1v"><h3 class="svelte-pnpw1v">Soft Newline Contexts</h3> `);
  if (each_array_1.length !== 0) {
    $$payload.out.push("<!--[-->");
    for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
      let [nodeId, context] = each_array_1[$$index_1];
      $$payload.out.push(`<div${attr_class("context-item svelte-pnpw1v", void 0, { "active": context.hasMarkdownAfterNewline })}><div class="context-header svelte-pnpw1v"><span class="node-id svelte-pnpw1v">Node: ${escape_html(nodeId.replace("demo-", "#"))}</span> `);
      if (context.hasMarkdownAfterNewline) {
        $$payload.out.push("<!--[-->");
        $$payload.out.push(`<span class="pattern-badge svelte-pnpw1v">${escape_html(formatPatternType(context.detectedPattern?.type || ""))}</span>`);
      } else {
        $$payload.out.push("<!--[!-->");
      }
      $$payload.out.push(`<!--]--></div> `);
      if (context.hasMarkdownAfterNewline && context.detectedPattern) {
        $$payload.out.push("<!--[-->");
        $$payload.out.push(`<div class="context-details svelte-pnpw1v"><div class="content-split svelte-pnpw1v"><div class="content-before svelte-pnpw1v"><strong>Before:</strong> "${escape_html(context.contentBefore)}"</div> <div class="content-after svelte-pnpw1v"><strong>After:</strong> "${escape_html(context.contentAfter)}"</div></div> <div class="pattern-info svelte-pnpw1v"><strong>Pattern:</strong> ${escape_html(context.detectedPattern.syntax)} → "${escape_html(context.detectedPattern.content)}"</div> `);
        if (context.suggestedNodeType) {
          $$payload.out.push("<!--[-->");
          $$payload.out.push(`<div class="suggestion-info svelte-pnpw1v"><strong>Suggests:</strong> ${escape_html(formatNodeType(context.suggestedNodeType))}</div>`);
        } else {
          $$payload.out.push("<!--[!-->");
        }
        $$payload.out.push(`<!--]--></div>`);
      } else {
        $$payload.out.push("<!--[!-->");
        $$payload.out.push(`<div class="no-context svelte-pnpw1v">No markdown patterns detected</div>`);
      }
      $$payload.out.push(`<!--]--></div>`);
    }
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<div class="empty-state svelte-pnpw1v">No soft newline contexts yet</div>`);
  }
  $$payload.out.push(`<!--]--></div> <div class="suggestions-panel svelte-pnpw1v"><div class="suggestions-header svelte-pnpw1v"><h3 class="svelte-pnpw1v">Node Creation Suggestions</h3> `);
  if (suggestions.length > 0) {
    $$payload.out.push("<!--[-->");
    $$payload.out.push(`<button class="clear-btn svelte-pnpw1v">Clear All</button>`);
  } else {
    $$payload.out.push("<!--[!-->");
  }
  $$payload.out.push(`<!--]--></div> `);
  if (each_array_2.length !== 0) {
    $$payload.out.push("<!--[-->");
    for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
      let suggestion = each_array_2[$$index_2];
      $$payload.out.push(`<div class="suggestion-item svelte-pnpw1v"><div class="suggestion-header svelte-pnpw1v"><span class="suggestion-type svelte-pnpw1v">${escape_html(formatNodeType(suggestion.nodeType))}</span> <span class="suggestion-relationship svelte-pnpw1v">(${escape_html(suggestion.relationship)})</span></div> <div class="suggestion-content svelte-pnpw1v">"${escape_html(suggestion.content)}"</div> <div class="suggestion-actions svelte-pnpw1v"><button class="accept-btn svelte-pnpw1v">✅ Accept</button> <button class="dismiss-btn svelte-pnpw1v">❌ Dismiss</button></div></div>`);
    }
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<div class="empty-state svelte-pnpw1v">No suggestions yet</div>`);
  }
  $$payload.out.push(`<!--]--></div> <div class="activity-panel svelte-pnpw1v"><h3 class="svelte-pnpw1v">Activity Log</h3> <div class="activity-log svelte-pnpw1v">`);
  if (each_array_3.length !== 0) {
    $$payload.out.push("<!--[-->");
    for (let $$index_3 = 0, $$length = each_array_3.length; $$index_3 < $$length; $$index_3++) {
      let logEntry = each_array_3[$$index_3];
      $$payload.out.push(`<div class="log-entry svelte-pnpw1v">${escape_html(logEntry)}</div>`);
    }
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<div class="empty-state svelte-pnpw1v">No activity yet</div>`);
  }
  $$payload.out.push(`<!--]--></div></div></div></div></div>`);
  pop();
}
function MockPositioningTest($$payload, $$props) {
  push();
  let passedTests, failedTests, avgPerformance, performancePassed;
  let testResults = [];
  let performanceResults = [];
  let activeTests = [];
  passedTests = testResults.filter((t) => t.passed).length;
  failedTests = testResults.filter((t) => !t.passed).length;
  avgPerformance = performanceResults.length > 0 ? performanceResults.reduce((acc, p) => acc + p.duration, 0) / performanceResults.length : 0;
  performancePassed = performanceResults.filter((p) => p.passed).length;
  let $$settled = true;
  let $$inner_payload;
  function $$render_inner($$payload2) {
    const each_array = ensure_array_like(activeTests);
    $$payload2.out.push(`<div class="mock-positioning-test svelte-k58394"><h2 class="svelte-k58394">Mock Element Positioning System Test</h2> <div class="test-controls svelte-k58394"><button class="run-tests-btn svelte-k58394">Run Positioning Tests</button> `);
    if (testResults.length > 0) {
      $$payload2.out.push("<!--[-->");
      $$payload2.out.push(`<div class="test-summary svelte-k58394"><span class="summary-item passed svelte-k58394">✅ Passed: ${escape_html(passedTests)}</span> <span class="summary-item failed svelte-k58394">❌ Failed: ${escape_html(failedTests)}</span> <span class="summary-item performance svelte-k58394">⚡ Avg: ${escape_html(avgPerformance.toFixed(2))}ms</span> <span class="summary-item performance svelte-k58394">🚀 Performance: ${escape_html(performancePassed)}/${escape_html(performanceResults.length)}</span></div>`);
    } else {
      $$payload2.out.push("<!--[!-->");
    }
    $$payload2.out.push(`<!--]--></div> <div class="test-scenarios svelte-k58394"><h3>Interactive Test Scenarios</h3> <p>Click on the text content below to test cursor positioning accuracy:</p> <!--[-->`);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let test = each_array[$$index];
      $$payload2.out.push(`<div class="test-scenario svelte-k58394"${attr_style(test.scenario.nodeStyle)}><h4 class="svelte-k58394">${escape_html(test.scenario.name)}</h4> <div class="test-node-container svelte-k58394">`);
      BaseNode($$payload2, {
        nodeId: test.id,
        nodeType: "text",
        editable: true,
        contentEditable: true,
        multiline: test.scenario.multiline,
        placeholder: "Click to test positioning...",
        get content() {
          return test.scenario.content;
        },
        set content($$value) {
          test.scenario.content = $$value;
          $$settled = false;
        }
      });
      $$payload2.out.push(`<!----></div> <div class="scenario-info svelte-k58394"><small>Font Size: ${escape_html(test.scenario.fontSize)} | Multiline: ${escape_html(test.scenario.multiline ? "Yes" : "No")} | Content Length: ${escape_html(test.scenario.content.length)} chars</small></div></div>`);
    }
    $$payload2.out.push(`<!--]--></div> `);
    if (testResults.length > 0) {
      $$payload2.out.push("<!--[-->");
      const each_array_1 = ensure_array_like(testResults);
      const each_array_2 = ensure_array_like(performanceResults);
      $$payload2.out.push(`<div class="test-results svelte-k58394"><h3>Test Results</h3> <div class="results-grid svelte-k58394"><!--[-->`);
      for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
        let result = each_array_1[$$index_1];
        $$payload2.out.push(`<div${attr_class("result-card svelte-k58394", void 0, { "passed": result.passed, "failed": !result.passed })}><div class="result-header svelte-k58394"><span class="result-status">${escape_html(result.passed ? "✅" : "❌")}</span> <span class="result-name">${escape_html(result.name)}</span></div> <div class="result-details"><div class="result-meta svelte-k58394"><span>Font: ${escape_html(result.fontSize)}</span> <span>Multi: ${escape_html(result.multiline ? "Yes" : "No")}</span></div> <div class="result-content svelte-k58394">${escape_html(result.content)}</div> <div class="result-message svelte-k58394">${escape_html(result.details)}</div></div></div>`);
      }
      $$payload2.out.push(`<!--]--></div></div> <div class="performance-results svelte-k58394"><h3>Performance Results</h3> <div class="performance-grid svelte-k58394"><!--[-->`);
      for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
        let perf = each_array_2[$$index_2];
        $$payload2.out.push(`<div${attr_class("perf-card svelte-k58394", void 0, { "passed": perf.passed, "failed": !perf.passed })}><span class="perf-scenario svelte-k58394">${escape_html(perf.scenario)}</span> <span class="perf-duration svelte-k58394">${escape_html(perf.duration.toFixed(2))}ms</span> <span class="perf-status svelte-k58394">${escape_html(perf.passed ? "✅" : "❌")}</span></div>`);
      }
      $$payload2.out.push(`<!--]--></div> <div class="performance-summary svelte-k58394"><p class="svelte-k58394"><strong>Performance Target:</strong> &lt; 50ms per positioning calculation</p> <p class="svelte-k58394"><strong>Average Performance:</strong> ${escape_html(avgPerformance.toFixed(2))}ms</p> <p class="svelte-k58394"><strong>Tests Passing:</strong> ${escape_html(performancePassed)}/${escape_html(performanceResults.length)}</p></div></div>`);
    } else {
      $$payload2.out.push("<!--[!-->");
    }
    $$payload2.out.push(`<!--]--></div>`);
  }
  do {
    $$settled = true;
    $$inner_payload = copy_payload($$payload);
    $$render_inner($$inner_payload);
  } while (!$$settled);
  assign_payload($$payload, $$inner_payload);
  pop();
}
function cn(...inputs) {
  return twMerge(clsx$1(inputs));
}
const buttonVariants = tv({
  base: "focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium outline-none transition-all focus-visible:ring-[3px] disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
  variants: {
    variant: {
      default: "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90",
      destructive: "bg-destructive shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60 text-white",
      outline: "bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50 border",
      secondary: "bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80",
      ghost: "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
      link: "text-primary underline-offset-4 hover:underline"
    },
    size: {
      default: "h-9 px-4 py-2 has-[>svg]:px-3",
      sm: "h-8 gap-1.5 rounded-md px-3 has-[>svg]:px-2.5",
      lg: "h-10 rounded-md px-6 has-[>svg]:px-4",
      icon: "size-9"
    }
  },
  defaultVariants: { variant: "default", size: "default" }
});
function Button($$payload, $$props) {
  push();
  let {
    class: className,
    variant = "default",
    size = "default",
    ref = null,
    href = void 0,
    type = "button",
    disabled,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  if (href) {
    $$payload.out.push("<!--[-->");
    $$payload.out.push(`<a${spread_attributes(
      {
        "data-slot": "button",
        class: clsx(cn(buttonVariants({ variant, size }), className)),
        href: disabled ? void 0 : href,
        "aria-disabled": disabled,
        role: disabled ? "link" : void 0,
        tabindex: disabled ? -1 : void 0,
        ...restProps
      }
    )}>`);
    children?.($$payload);
    $$payload.out.push(`<!----></a>`);
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<button${spread_attributes(
      {
        "data-slot": "button",
        class: clsx(cn(buttonVariants({ variant, size }), className)),
        type,
        disabled,
        ...restProps
      }
    )}>`);
    children?.($$payload);
    $$payload.out.push(`<!----></button>`);
  }
  $$payload.out.push(`<!--]-->`);
  bind_props($$props, { ref });
  pop();
}
function Input($$payload, $$props) {
  push();
  let {
    ref = null,
    value = void 0,
    type,
    files = void 0,
    class: className,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  if (type === "file") {
    $$payload.out.push("<!--[-->");
    $$payload.out.push(`<input${spread_attributes(
      {
        "data-slot": "input",
        class: clsx(cn("selection:bg-primary dark:bg-input/30 selection:text-primary-foreground border-input ring-offset-background placeholder:text-muted-foreground shadow-xs flex h-9 w-full min-w-0 rounded-md border bg-transparent px-3 pt-1.5 text-sm font-medium outline-none transition-[color,box-shadow] disabled:cursor-not-allowed disabled:opacity-50 md:text-sm", "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]", "aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive", className)),
        type: "file",
        ...restProps
      }
    )}/>`);
  } else {
    $$payload.out.push("<!--[!-->");
    $$payload.out.push(`<input${spread_attributes(
      {
        "data-slot": "input",
        class: clsx(cn("border-input bg-background selection:bg-primary dark:bg-input/30 selection:text-primary-foreground ring-offset-background placeholder:text-muted-foreground shadow-xs flex h-9 w-full min-w-0 rounded-md border px-3 py-1 text-base outline-none transition-[color,box-shadow] disabled:cursor-not-allowed disabled:opacity-50 md:text-sm", "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]", "aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive", className)),
        type,
        value,
        ...restProps
      }
    )}/>`);
  }
  $$payload.out.push(`<!--]-->`);
  bind_props($$props, { ref, value, files });
  pop();
}
function Card($$payload, $$props) {
  push();
  let {
    ref = null,
    class: className,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  $$payload.out.push(`<div${spread_attributes(
    {
      "data-slot": "card",
      class: clsx(cn("bg-card text-card-foreground flex flex-col gap-6 rounded-xl border py-6 shadow-sm", className)),
      ...restProps
    }
  )}>`);
  children?.($$payload);
  $$payload.out.push(`<!----></div>`);
  bind_props($$props, { ref });
  pop();
}
function Card_content($$payload, $$props) {
  push();
  let {
    ref = null,
    class: className,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  $$payload.out.push(`<div${spread_attributes(
    {
      "data-slot": "card-content",
      class: clsx(cn("px-6", className)),
      ...restProps
    }
  )}>`);
  children?.($$payload);
  $$payload.out.push(`<!----></div>`);
  bind_props($$props, { ref });
  pop();
}
function Card_description($$payload, $$props) {
  push();
  let {
    ref = null,
    class: className,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  $$payload.out.push(`<p${spread_attributes(
    {
      "data-slot": "card-description",
      class: clsx(cn("text-muted-foreground text-sm", className)),
      ...restProps
    }
  )}>`);
  children?.($$payload);
  $$payload.out.push(`<!----></p>`);
  bind_props($$props, { ref });
  pop();
}
function Card_header($$payload, $$props) {
  push();
  let {
    ref = null,
    class: className,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  $$payload.out.push(`<div${spread_attributes(
    {
      "data-slot": "card-header",
      class: clsx(cn("@container/card-header has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6 grid auto-rows-min grid-rows-[auto_auto] items-start gap-1.5 px-6", className)),
      ...restProps
    }
  )}>`);
  children?.($$payload);
  $$payload.out.push(`<!----></div>`);
  bind_props($$props, { ref });
  pop();
}
function Card_title($$payload, $$props) {
  push();
  let {
    ref = null,
    class: className,
    children,
    $$slots,
    $$events,
    ...restProps
  } = $$props;
  $$payload.out.push(`<div${spread_attributes(
    {
      "data-slot": "card-title",
      class: clsx(cn("font-semibold leading-none", className)),
      ...restProps
    }
  )}>`);
  children?.($$payload);
  $$payload.out.push(`<!----></div>`);
  bind_props($$props, { ref });
  pop();
}
function _page($$payload, $$props) {
  push();
  var $$store_subs;
  let appName = "NodeSpace";
  let appVersion = "0.1.0";
  let testMessage = "";
  async function testConnection() {
    try {
      testMessage = await invoke("greet", { name: "NodeSpace" });
    } catch (error) {
      testMessage = "Connection test failed";
      console.error("Test failed:", error);
    }
  }
  function toggleTheme2() {
    const current = store_get($$store_subs ??= {}, "$themePreference", themePreference);
    if (current === "system") {
      themePreference.set(store_get($$store_subs ??= {}, "$currentTheme", currentTheme) === "dark" ? "light" : "dark");
    } else if (current === "light") {
      themePreference.set("dark");
    } else {
      themePreference.set("light");
    }
  }
  function getThemeIcon(theme) {
    switch (theme) {
      case "light":
        return "☀️";
      case "dark":
        return "🌙";
      case "system":
        return "🖥️";
      default:
        return "🖥️";
    }
  }
  ThemeProvider($$payload, {
    children: ($$payload2) => {
      $$payload2.out.push(`<main class="nodespace-app svelte-1s2q9l3"><header class="app-header svelte-1s2q9l3"><h1 class="svelte-1s2q9l3">${escape_html(appName)}</h1> <div class="app-info svelte-1s2q9l3"><span class="version svelte-1s2q9l3">v${escape_html(appVersion)}</span> `);
      Button($$payload2, {
        variant: "outline",
        size: "sm",
        onclick: toggleTheme2,
        title: `Toggle theme (${stringify(store_get($$store_subs ??= {}, "$themePreference", themePreference))})`,
        children: ($$payload3) => {
          $$payload3.out.push(`<!---->${escape_html(getThemeIcon(store_get($$store_subs ??= {}, "$themePreference", themePreference)))}`);
        },
        $$slots: { default: true }
      });
      $$payload2.out.push(`<!----> `);
      Button($$payload2, {
        variant: "default",
        size: "sm",
        onclick: testConnection,
        children: ($$payload3) => {
          $$payload3.out.push(`<!---->Test Connection`);
        },
        $$slots: { default: true }
      });
      $$payload2.out.push(`<!----></div></header> <div class="app-layout svelte-1s2q9l3"><aside class="sidebar svelte-1s2q9l3"><div class="ns-panel journal-view svelte-1s2q9l3"><h3 class="svelte-1s2q9l3">JournalView</h3> <p class="svelte-1s2q9l3">Hierarchical note organization panel</p> <div class="placeholder-content svelte-1s2q9l3">`);
      TextNode($$payload2, {
        nodeId: "daily-notes",
        content: "**Daily Notes**\n\nToday's thoughts and observations",
        compact: true
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "projects",
        content: "**Projects**\n\nActive project documentation",
        compact: true
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "ideas",
        content: "**Ideas**\n\nCreative concepts and inspiration",
        compact: true
      });
      $$payload2.out.push(`<!----></div></div> <div class="ns-panel library-view svelte-1s2q9l3"><h3 class="svelte-1s2q9l3">LibraryView</h3> <p class="svelte-1s2q9l3">Knowledge base and documents</p> <div class="placeholder-content svelte-1s2q9l3">`);
      TextNode($$payload2, {
        nodeId: "templates",
        content: "**Templates**\n\nReusable document templates",
        compact: true
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "saved-searches",
        content: "**Saved Searches**\n\nFrequently used search queries",
        compact: true
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "reports",
        content: "**Reports**\n\nGenerated analytics and insights",
        compact: true
      });
      $$payload2.out.push(`<!----></div></div></aside> <main class="main-content svelte-1s2q9l3"><div class="ns-panel node-viewer svelte-1s2q9l3"><h3 class="svelte-1s2q9l3">NodeViewer</h3> <p class="svelte-1s2q9l3">Main content editing and viewing area with hierarchical display patterns</p> `);
      Card($$payload2, {
        class: "showcase-card",
        children: ($$payload3) => {
          Card_header($$payload3, {
            children: ($$payload4) => {
              Card_title($$payload4, {
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->shadcn-svelte Components`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----> `);
              Card_description($$payload4, {
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Professional UI components using your NodeSpace color scheme`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!---->`);
            },
            $$slots: { default: true }
          });
          $$payload3.out.push(`<!----> `);
          Card_content($$payload3, {
            class: "component-showcase",
            children: ($$payload4) => {
              $$payload4.out.push(`<div class="showcase-section svelte-1s2q9l3"><h4 class="svelte-1s2q9l3">Buttons</h4> <div class="showcase-row svelte-1s2q9l3">`);
              Button($$payload4, {
                variant: "default",
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Primary`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----> `);
              Button($$payload4, {
                variant: "secondary",
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Secondary`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----> `);
              Button($$payload4, {
                variant: "outline",
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Outline`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----> `);
              Button($$payload4, {
                variant: "ghost",
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Ghost`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----></div></div> <div class="showcase-section svelte-1s2q9l3"><h4 class="svelte-1s2q9l3">Form Controls</h4> <div class="showcase-row svelte-1s2q9l3">`);
              Input($$payload4, { placeholder: "Enter text..." });
              $$payload4.out.push(`<!----> `);
              Button($$payload4, {
                variant: "default",
                children: ($$payload5) => {
                  $$payload5.out.push(`<!---->Submit`);
                },
                $$slots: { default: true }
              });
              $$payload4.out.push(`<!----></div></div>`);
            },
            $$slots: { default: true }
          });
          $$payload3.out.push(`<!---->`);
        },
        $$slots: { default: true }
      });
      $$payload2.out.push(`<!----> `);
      TextNodeDemo($$payload2);
      $$payload2.out.push(`<!----> <div class="divider svelte-1s2q9l3"></div> `);
      BulletConversionDemo($$payload2);
      $$payload2.out.push(`<!----> <div class="divider svelte-1s2q9l3"></div> `);
      SoftNewlineDemo($$payload2);
      $$payload2.out.push(`<!----> <div class="divider svelte-1s2q9l3"></div> `);
      HierarchyDemo($$payload2);
      $$payload2.out.push(`<!----> <div class="editor-placeholder svelte-1s2q9l3"><p class="svelte-1s2q9l3">TextNode examples with enhanced editing and markdown support:</p> <div class="node-examples svelte-1s2q9l3">`);
      TextNode($$payload2, {
        nodeId: "example-text",
        content: "**TextNode Example (Childless)**\n\nThis is an example of a text node with **markdown support** and rich formatting capabilities. Notice the enhanced TextNode with inline editing functionality."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-text-parent",
        content: "**TextNode Example (Parent)**\n\nThis is an example of a parent text node that contains child nodes with rich markdown support."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-task",
        content: "**TaskNode Example (Childless)**\n\n☐ Complete design system implementation\\n☑ Set up Tauri app structure\\n☐ Add AI integration\\n\\nClick to edit and experience the enhanced TextNode functionality."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-task-parent",
        content: "**TaskNode Example (Parent)**\n\n☐ Complete design system implementation\\n☑ Set up Tauri app structure\\n☐ Add AI integration\\n\\nThis is a parent task node with subtasks."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-ai-chat",
        content: "**AIChatNode Example (Childless)**\n\nAI Assistant: How can I help you organize your knowledge today?\\n\\nThis enhanced TextNode supports inline editing with auto-save functionality."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-ai-chat-parent",
        content: "**AIChatNode Example (Parent)**\n\nAI Assistant: How can I help you organize your knowledge today?\\n\\nThis is a conversation with follow-ups and enhanced TextNode capabilities."
      });
      $$payload2.out.push(`<!----> `);
      TextNode($$payload2, {
        nodeId: "example-icon-override",
        content: "**Enhanced TextNode Features**\n\nThis TextNode demonstrates the enhanced inline editing capabilities with markdown support, auto-save functionality, and responsive panel behavior."
      });
      $$payload2.out.push(`<!----></div></div> <div class="positioning-test svelte-1s2q9l3">`);
      MockPositioningTest($$payload2);
      $$payload2.out.push(`<!----></div></div></main> <aside class="right-sidebar svelte-1s2q9l3"><div class="ns-panel ai-chat-view svelte-1s2q9l3"><h3 class="svelte-1s2q9l3">AIChatView</h3> <p class="svelte-1s2q9l3">AI assistant interaction panel</p> <div class="chat-placeholder svelte-1s2q9l3">`);
      TextNode($$payload2, {
        nodeId: "chat-example",
        content: "**AI Conversation**\n\nHow can I organize my notes effectively?",
        compact: true
      });
      $$payload2.out.push(`<!----> <div class="chat-input-area svelte-1s2q9l3">`);
      Input($$payload2, {
        type: "text",
        placeholder: "Ask AI anything...",
        disabled: true
      });
      $$payload2.out.push(`<!----> `);
      Button($$payload2, {
        variant: "default",
        size: "sm",
        disabled: true,
        children: ($$payload3) => {
          $$payload3.out.push(`<!---->Send`);
        },
        $$slots: { default: true }
      });
      $$payload2.out.push(`<!----></div></div></div></aside></div> <footer class="app-footer svelte-1s2q9l3"><div class="status-bar svelte-1s2q9l3"><span class="status svelte-1s2q9l3">Ready</span> `);
      if (testMessage) {
        $$payload2.out.push("<!--[-->");
        $$payload2.out.push(`<span class="test-result svelte-1s2q9l3">${escape_html(testMessage)}</span>`);
      } else {
        $$payload2.out.push("<!--[!-->");
      }
      $$payload2.out.push(`<!--]--> <span class="theme-indicator svelte-1s2q9l3">Theme: ${escape_html(store_get($$store_subs ??= {}, "$themePreference", themePreference))} (${escape_html(store_get($$store_subs ??= {}, "$currentTheme", currentTheme))})</span></div></footer></main>`);
    },
    $$slots: { default: true }
  });
  if ($$store_subs) unsubscribe_stores($$store_subs);
  pop();
}
export {
  _page as default
};
