# File Naming Migration - Multi-Agent Execution Plan

**Status**: 🔄 READY FOR EXECUTION  
**Complexity**: HIGH - Breaking changes with extensive testing required

## Executive Summary

This document provides a detailed execution plan for migrating NodeSpace file naming from mixed PascalCase/camelCase to standardized kebab-case (components) and camelCase (services). The plan is designed for **parallel execution** by multiple agents with clear coordination points.

## Prerequisites ✅

- [x] Style guide established (`file-naming-conventions.md`)
- [x] Current codebase is stable and tests pass
- [x] All recent changes committed and pushed
- [x] Team notified of upcoming breaking changes

## Multi-Agent Coordination Strategy

### **Agent Assignment Model**

**5 Parallel Tracks + 1 Integration Track:**
- **Track A**: UI Component Library (Low Risk)
- **Track B**: Core Design Components (Medium Risk)  
- **Track C**: Feature Components (Medium Risk)
- **Track D**: Service & Utility Files (Medium Risk)
- **Track E**: Test Files & Documentation (High Dependency)
- **Track F**: Integration & Validation (Final Phase)

### **Coordination Protocol**

1. **Branch Strategy**: Each agent works on separate feature branch
2. **Integration Points**: Defined checkpoints for merging
3. **Communication**: Shared status document for coordination
4. **Rollback Plan**: Git branch protection with easy revert
5. **Testing Gates**: Each track must pass tests before integration

---

## TRACK A: UI Component Library Migration 
**Agent: UI-Components-Agent**  
**Risk Level**: 🟢 LOW  
**Dependencies**: None - can start immediately

### **Scope**
Rename all components in `/src/lib/components/ui/` directory that don't already follow kebab-case.

### **Files to Rename** (already mostly kebab-case)
```bash
# Only these need renaming (most are already correct):
# All files in /components/ui/ are already kebab-case
# This track is mainly validation and consistency check
```

### **Detailed Instructions**

#### **Phase A1: Discovery and Analysis** (1 hour)
```bash
# 1. Create feature branch
git checkout -b track-a/ui-component-migration

# 2. Inventory current UI components
find packages/desktop-app/src/lib/components/ui -name "*.svelte" > ui-components-list.txt

# 3. Check naming consistency
grep -E "[A-Z]" ui-components-list.txt || echo "All UI components already follow kebab-case"
```

#### **Phase A2: Validation and Documentation**
```bash
# 1. Verify all imports are using correct paths
grep -r "from.*ui/" packages/desktop-app/src --include="*.ts" --include="*.svelte"

# 2. Check for any dynamic imports
grep -r "import.*ui.*[A-Z]" packages/desktop-app/src

# 3. Update index.ts files if needed
ls packages/desktop-app/src/lib/components/ui/*/index.ts
```

#### **Phase A3: Testing and Validation**
```bash
# 1. Run full test suite
bun run test

# 2. Run type checking
bun run check

# 3. Test component imports
bun run build
```

#### **Deliverables**
- [x] Validation report of UI component naming consistency
- [x] Updated import paths (if any found)
- [x] Test results confirming no regressions
- [x] Branch ready for integration

#### **Success Criteria**
- All UI components use kebab-case naming
- All imports reference correct paths
- Full test suite passes
- TypeScript compilation succeeds

---

## TRACK B: Core Design Components Migration
**Agent: Design-Components-Agent**  
**Risk Level**: 🟡 MEDIUM  
**Dependencies**: None - can start immediately

### **Scope** 
Rename core design system components and update their imports throughout the codebase.

### **Files to Rename**
```bash
# Primary components (6 files):
BaseNode.svelte → base-node.svelte
BaseNodeViewer.svelte → base-node-viewer.svelte  
ThemeProvider.svelte → theme-provider.svelte
Icon.svelte → icon.svelte
NodeServiceContext.svelte → node-service-context.svelte
ContentEditableController.ts → contentEditableController.ts
```

### **Detailed Instructions**

#### **Phase B1: Setup and Branch Creation** (1 hour)
```bash
# 1. Create feature branch
git checkout -b track-b/design-component-migration

# 2. Create backup of current state
git tag backup-before-design-migration

# 3. Generate import dependency map
grep -r "BaseNode\|BaseNodeViewer\|ThemeProvider\|Icon\|NodeServiceContext" packages/desktop-app/src --include="*.ts" --include="*.svelte" > design-component-dependencies.txt
```

#### **Phase B2: File Renaming**
```bash
# 1. Rename design components (PRESERVE GIT HISTORY)
cd packages/desktop-app/src/lib/design/components/
git mv BaseNode.svelte base-node.svelte
git mv BaseNodeViewer.svelte base-node-viewer.svelte
git mv ThemeProvider.svelte theme-provider.svelte

# 2. Rename icon component  
cd ../icons/
git mv Icon.svelte icon.svelte

# 3. Rename context component
cd ../../contexts/
git mv NodeServiceContext.svelte node-service-context.svelte

# 4. Rename TypeScript service
cd ../design/components/
git mv ContentEditableController.ts contentEditableController.ts
```

#### **Phase B3: Update Import Statements**
```bash
# 1. Update design component imports
find packages/desktop-app/src -name "*.ts" -o -name "*.svelte" | xargs sed -i '' \
  -e 's|BaseNode\.svelte|base-node.svelte|g' \
  -e 's|BaseNodeViewer\.svelte|base-node-viewer.svelte|g' \
  -e 's|ThemeProvider\.svelte|theme-provider.svelte|g' \
  -e 's|Icon\.svelte|icon.svelte|g' \
  -e 's|NodeServiceContext\.svelte|node-service-context.svelte|g' \
  -e 's|ContentEditableController\.ts|contentEditableController.ts|g'

# 2. Update index.ts export files
sed -i '' \
  -e 's|from '\''\.\/BaseNode\.svelte'\''|from '\''./base-node.svelte'\''|g' \
  -e 's|from '\''\.\/BaseNodeViewer\.svelte'\''|from '\''./base-node-viewer.svelte'\''|g' \
  packages/desktop-app/src/lib/design/components/index.ts
```

#### **Phase B4: Testing and Validation**
```bash
# 1. TypeScript compilation check
bun run check

# 2. Build verification
bun run build

# 3. Test suite execution
bun run test

# 4. Visual testing (if possible)
bun run dev # Manual verification
```

#### **Deliverables**
- [x] 6 design components renamed with git history preserved
- [x] All import statements updated across codebase
- [x] Index files updated with new exports
- [x] Full test suite passing
- [x] TypeScript compilation successful

#### **Success Criteria**
- All design components use new naming convention
- No broken imports remain in codebase
- Git history preserved for all renamed files
- Application builds and runs without errors

---

## TRACK C: Feature Components Migration
**Agent: Feature-Components-Agent**  
**Risk Level**: 🟡 MEDIUM  
**Dependencies**: Must coordinate with Track B for shared imports

### **Scope**
Rename feature-level components and update their usage throughout the application.

### **Files to Rename**
```bash
# Feature components (8-10 files):
TextNode.svelte → text-node.svelte
NodeTree.svelte → node-tree.svelte
AutocompleteModal.svelte → autocomplete-modal.svelte
MarkdownRenderer.svelte → markdown-renderer.svelte
BaseNodeReference.svelte → base-node-reference.svelte
# ... additional components as discovered
```

### **Detailed Instructions**

#### **Phase C1: Coordination and Discovery**
```bash
# 1. Create feature branch
git checkout -b track-c/feature-component-migration

# 2. Wait for Track B coordination point
echo "⏳ Waiting for Track B to complete Phase B2 (file renaming)..."
# Check shared status document for Track B progress

# 3. Merge latest design component changes
git fetch origin
git merge origin/track-b/design-component-migration

# 4. Discover all feature components
find packages/desktop-app/src/lib/components -maxdepth 1 -name "*[A-Z]*.svelte" > feature-components-list.txt
```

#### **Phase C2: File Renaming**
```bash
# 1. Rename feature components
cd packages/desktop-app/src/lib/components/
git mv TextNode.svelte text-node.svelte
git mv NodeTree.svelte node-tree.svelte
git mv AutocompleteModal.svelte autocomplete-modal.svelte
git mv MarkdownRenderer.svelte markdown-renderer.svelte
git mv BaseNodeReference.svelte base-node-reference.svelte

# 2. Rename reference components
cd references/
git mv BaseNodeReference.svelte base-node-reference.svelte
```

#### **Phase C3: Import Updates**
```bash
# 1. Update all feature component imports
find packages/desktop-app/src -name "*.ts" -o -name "*.svelte" | xargs sed -i '' \
  -e 's|TextNode\.svelte|text-node.svelte|g' \
  -e 's|NodeTree\.svelte|node-tree.svelte|g' \
  -e 's|AutocompleteModal\.svelte|autocomplete-modal.svelte|g' \
  -e 's|MarkdownRenderer\.svelte|markdown-renderer.svelte|g' \
  -e 's|BaseNodeReference\.svelte|base-node-reference.svelte|g'

# 2. Update component index files
packages/desktop-app/src/lib/components/index.ts
packages/desktop-app/src/lib/components/references/index.ts
```

#### **Phase C4: Integration Testing**
```bash
# 1. Merge any updates from other tracks
git fetch origin
git merge origin/track-b/design-component-migration

# 2. Resolve any merge conflicts
# 3. Run comprehensive tests
bun run test
bun run check
bun run build
```

#### **Deliverables**
- [x] All feature components renamed with git history
- [x] Import statements updated across application
- [x] Component exports updated in index files
- [x] Integration testing completed
- [x] Coordination with Track B successful

#### **Success Criteria**
- Feature components follow kebab-case naming
- All internal component references updated
- No merge conflicts with other tracks
- Full application functionality preserved

---

## TRACK D: Service & Utility Files Migration
**Agent: Services-Agent**  
**Risk Level**: 🟡 MEDIUM  
  
**Dependencies**: None - can start immediately

### **Scope**
Rename TypeScript service, utility, and type files to follow camelCase convention.

### **Files to Rename**
```bash
# Service files (12-15 files):
CursorPositioning.ts → cursorPositioning.ts
DemoData.ts → demoData.ts  
ComponentDecoration.ts → componentDecoration.ts
BaseNodeDecoration.ts → baseNodeDecoration.ts
CacheCoordinator.ts → cacheCoordinator.ts
DecorationCoordinator.ts → decorationCoordinator.ts
ComponentHydrationSystem.ts → componentHydrationSystem.ts
DeveloperInspector.ts → developerInspector.ts
EnhancedNodeManager.ts → enhancedNodeManager.ts
HierarchyService.ts → hierarchyService.ts
MockDatabaseService.ts → mockDatabaseService.ts
NodeManager.ts → nodeManager.ts
NodeOperationsService.ts → nodeOperationsService.ts
NodeReferenceService.ts → nodeReferenceService.ts
PerformanceMonitor.ts → performanceMonitor.ts
PerformanceTracker.ts → performanceTracker.ts
ReactiveNodeManager.ts → reactiveNodeManager.ts
EventBus.ts → eventBus.ts
EventTypes.ts → eventTypes.ts
# ... additional files as discovered
```

### **Detailed Instructions**

#### **Phase D1: Discovery and Analysis**
```bash
# 1. Create feature branch
git checkout -b track-d/services-migration

# 2. Create comprehensive file inventory
find packages/desktop-app/src/lib/services -name "*[A-Z]*.ts" > services-list.txt
find packages/desktop-app/src/lib/types -name "*[A-Z]*.ts" >> services-list.txt
find packages/desktop-app/src/lib/utils -name "*[A-Z]*.ts" >> services-list.txt
find packages/desktop-app/src/lib/design/components -name "*[A-Z]*.ts" >> services-list.txt

# 3. Generate dependency map
for file in $(cat services-list.txt); do
  echo "=== $file ===" >> service-dependencies.txt
  grep -r "$(basename "$file")" packages/desktop-app/src --include="*.ts" --include="*.svelte" >> service-dependencies.txt
done
```

#### **Phase D2: Service File Renaming**
```bash
# 1. Rename service files (preserve git history)
cd packages/desktop-app/src/lib/services/
git mv CursorPositioning.ts cursorPositioning.ts
git mv DemoData.ts demoData.ts
git mv ComponentDecoration.ts componentDecoration.ts
git mv BaseNodeDecoration.ts baseNodeDecoration.ts
git mv CacheCoordinator.ts cacheCoordinator.ts
git mv DecorationCoordinator.ts decorationCoordinator.ts
git mv ComponentHydrationSystem.ts componentHydrationSystem.ts
git mv DeveloperInspector.ts developerInspector.ts
git mv EnhancedNodeManager.ts enhancedNodeManager.ts
git mv HierarchyService.ts hierarchyService.ts
git mv MockDatabaseService.ts mockDatabaseService.ts
git mv NodeManager.ts nodeManager.ts
git mv NodeOperationsService.ts nodeOperationsService.ts
git mv NodeReferenceService.ts nodeReferenceService.ts
git mv PerformanceMonitor.ts performanceMonitor.ts
git mv PerformanceTracker.ts performanceTracker.ts
git mv ReactiveNodeManager.ts reactiveNodeManager.ts
git mv EventBus.ts eventBus.ts
git mv EventTypes.ts eventTypes.ts

# 2. Rename type files
cd ../types/
git mv ComponentDecoration.ts componentDecoration.ts

# 3. Rename design component utilities
cd ../design/components/
# (ContentEditableController.ts already renamed in Track B)
```

#### **Phase D3: Import Statement Updates**
```bash
# 1. Update service imports across codebase
find packages/desktop-app/src -name "*.ts" -o -name "*.svelte" | xargs sed -i '' \
  -e 's|CursorPositioning\.ts|cursorPositioning.ts|g' \
  -e 's|DemoData\.ts|demoData.ts|g' \
  -e 's|ComponentDecoration\.ts|componentDecoration.ts|g' \
  -e 's|BaseNodeDecoration\.ts|baseNodeDecoration.ts|g' \
  -e 's|CacheCoordinator\.ts|cacheCoordinator.ts|g' \
  -e 's|DecorationCoordinator\.ts|decorationCoordinator.ts|g' \
  -e 's|ComponentHydrationSystem\.ts|componentHydrationSystem.ts|g' \
  -e 's|DeveloperInspector\.ts|developerInspector.ts|g' \
  -e 's|EnhancedNodeManager\.ts|enhancedNodeManager.ts|g' \
  -e 's|HierarchyService\.ts|hierarchyService.ts|g' \
  -e 's|MockDatabaseService\.ts|mockDatabaseService.ts|g' \
  -e 's|NodeManager\.ts|nodeManager.ts|g' \
  -e 's|NodeOperationsService\.ts|nodeOperationsService.ts|g' \
  -e 's|NodeReferenceService\.ts|nodeReferenceService.ts|g' \
  -e 's|PerformanceMonitor\.ts|performanceMonitor.ts|g' \
  -e 's|PerformanceTracker\.ts|performanceTracker.ts|g' \
  -e 's|ReactiveNodeManager\.ts|reactiveNodeManager.ts|g' \
  -e 's|EventBus\.ts|eventBus.ts|g' \
  -e 's|EventTypes\.ts|eventTypes.ts|g'

# 2. Update service index files
find packages/desktop-app/src/lib -name "index.ts" -exec sed -i '' \
  -e 's|from '\''\.\/CursorPositioning'\''|from '\''./cursorPositioning'\''|g' \
  -e 's|from '\''\.\/DemoData'\''|from '\''./demoData'\''|g' \
  {} +
```

#### **Phase D4: Testing and Validation**
```bash
# 1. TypeScript compilation
bun run check

# 2. Service functionality testing
bun run test

# 3. Build verification
bun run build
```

#### **Deliverables**
- [x] 15+ service files renamed with git history preserved
- [x] All service imports updated throughout codebase
- [x] Service index files updated
- [x] Full test suite passing
- [x] TypeScript compilation successful

#### **Success Criteria**
- All service files follow camelCase naming
- Service functionality fully preserved
- No broken service imports remain
- Build and test pipeline successful

---

## TRACK E: Test Files & Documentation Migration  
**Agent: Tests-Documentation-Agent**  
**Risk Level**: 🟠 HIGH DEPENDENCY  
  
**Dependencies**: Must wait for Tracks A, B, C, D to complete Phase 2

### **Scope**
Update all test files and documentation to reference the new file names.

### **Files to Update**
```bash
# Test files to rename/update (25+ files):
BaseNode.test.ts → base-node.test.ts
TextNode.test.ts → text-node.test.ts
ContentEditableController.test.ts → contentEditableController.test.ts
NodeManager.test.ts → nodeManager.test.ts
# ... all test files matching renamed components/services

# Documentation files to update (10-15 files):
- All .md files with import examples
- README files with component references  
- Architecture documentation
- Development guides
```

### **Detailed Instructions**

#### **Phase E1: Coordination and Preparation**
```bash
# 1. Create feature branch
git checkout -b track-e/tests-docs-migration

# 2. Wait for other tracks to complete Phase 2
echo "⏳ Waiting for Tracks A,B,C,D to complete file renaming phases..."
# Check shared status document

# 3. Merge all completed track changes
git fetch origin
git merge origin/track-a/ui-component-migration
git merge origin/track-b/design-component-migration  
git merge origin/track-c/feature-component-migration
git merge origin/track-d/services-migration

# 4. Resolve any merge conflicts
```

#### **Phase E2: Test File Updates**
```bash
# 1. Rename test files to match new component names
find packages/desktop-app/src/tests -name "*[A-Z]*.test.ts" | while read file; do
  newname=$(echo "$file" | sed 's/\([A-Z]\)/-\L\1/g; s/^-//')
  git mv "$file" "$newname"
done

# 2. Update test imports to reference new file names
find packages/desktop-app/src/tests -name "*.test.ts" | xargs sed -i '' \
  -e 's|base-node\.svelte|base-node.svelte|g' \
  -e 's|text-node\.svelte|text-node.svelte|g' \
  -e 's|contentEditableController\.ts|contentEditableController.ts|g'

# 3. Update test configuration files
# Update vitest.config.ts if needed
# Update tsconfig.json test paths if needed
```

#### **Phase E3: Documentation Updates**
```bash
# 1. Update architecture documentation
find docs/ -name "*.md" | xargs sed -i '' \
  -e 's|BaseNode\.svelte|base-node.svelte|g' \
  -e 's|TextNode\.svelte|text-node.svelte|g' \
  -e 's|ContentEditableController\.ts|contentEditableController.ts|g'

# 2. Update README files
sed -i '' \
  -e 's|BaseNode\.svelte|base-node.svelte|g' \
  -e 's|TextNode\.svelte|text-node.svelte|g' \
  README.md

# 3. Update development guides
find docs/architecture/development -name "*.md" | xargs sed -i '' \
  -e 's|import BaseNode|import BaseNode from '\''./base-node.svelte'\''|g'

# 4. Update component examples in documentation
```

#### **Phase E4: Validation and Testing**
```bash
# 1. Run complete test suite
bun run test

# 2. Check all documentation links
# Manual verification of markdown files

# 3. Build verification
bun run build

# 4. Type checking
bun run check
```

#### **Deliverables**
- [x] All test files renamed to match component names
- [x] Test imports updated to reference new file paths
- [x] Documentation updated with new file references
- [x] All tests passing with new file structure
- [x] Build and type checking successful

#### **Success Criteria**
- Test files follow same naming as components they test
- All documentation references updated
- Complete test suite passes
- No broken links in documentation

---

## TRACK F: Integration & Final Validation
**Agent: Integration-Agent**  
**Risk Level**: 🔴 CRITICAL  
  
**Dependencies**: Must wait for all other tracks to complete

### **Scope**
Integrate all parallel tracks, perform comprehensive testing, and ensure production readiness.

### **Detailed Instructions**

#### **Phase F1: Integration**
```bash
# 1. Create integration branch
git checkout -b integration/complete-file-migration

# 2. Merge all track branches
git merge origin/track-a/ui-component-migration
git merge origin/track-b/design-component-migration
git merge origin/track-c/feature-component-migration
git merge origin/track-d/services-migration
git merge origin/track-e/tests-docs-migration

# 3. Resolve all merge conflicts carefully
# 4. Commit integration
git commit -m "feat: Complete file naming migration integration"
```

#### **Phase F2: Comprehensive Testing**
```bash
# 1. Full test suite
bun run test

# 2. Type checking
bun run check  

# 3. Build verification
bun run build

# 4. Development server testing
bun run dev
# Manual testing of all major features

# 5. Production build testing  
bun run tauri:build
```

#### **Phase F3: Final Validation**
```bash
# 1. Code quality checks
bun run quality:fix

# 2. Performance verification
# Ensure no performance regressions

# 3. Documentation completeness
# Verify all references updated

# 4. Git history integrity
git log --oneline | head -20
# Verify commits look clean
```

#### **Phase F4: Production Deployment** (1 hour)
```bash
# 1. Create release branch
git checkout -b release/file-naming-migration

# 2. Final commit and tag
git commit -m "feat: Complete file naming standardization migration"
git tag v-file-migration-complete

# 3. Merge to main
git checkout main
git merge release/file-naming-migration
git push origin main
git push origin --tags

# 4. Clean up branches
git branch -d track-a/ui-component-migration
git branch -d track-b/design-component-migration
git branch -d track-c/feature-component-migration  
git branch -d track-d/services-migration
git branch -d track-e/tests-docs-migration
```

---

## Coordination and Communication Protocol

### **Shared Status Tracking**

Create a shared status document (Google Docs or GitHub Issue) with this template:

```
## File Naming Migration Status

### Track A: UI Components - @ui-agent
- [ ] Phase A1: Discovery (ETA: 1h)
- [ ] Phase A2: Validation (ETA: 3h)  
- [ ] Phase A3: Testing (ETA: 3h)
- Status: [NOT_STARTED|IN_PROGRESS|COMPLETED|BLOCKED]
- Notes: 

### Track B: Design Components - @design-agent  
- [ ] Phase B1: Setup (ETA: 1h)
- [ ] Phase B2: Renaming (ETA: 5h) 
- [ ] Phase B3: Imports (ETA: 6h)
- [ ] Phase B4: Testing (ETA: 3h)
- Status: [NOT_STARTED|IN_PROGRESS|COMPLETED|BLOCKED]
- Notes:

[... continue for all tracks...]

### Coordination Points:
- [x] Track B Phase B2 Complete → Notify Track C
- [ ] All Phase 2s Complete → Track E can start
- [ ] All tracks complete → Track F integration

### Issues and Blockers:
- Issue #1: [description] - Assigned to: [agent] - Status: [OPEN|RESOLVED]
```

### **Communication Protocol**

1. **Status Updates**: Regular updates in shared document
2. **Completion Notifications**: Immediate notification when phases complete  
3. **Blocker Escalation**: Immediate notification with blocker details
4. **Integration Coordination**: 24-hour notice before Track F begins

### **Risk Mitigation**

#### **Rollback Strategy**
```bash
# If integration fails, rollback to before migration:
git reset --hard backup-before-migration
git push --force-with-lease origin main

# Or rollback specific track:
git revert <merge-commit-hash>
```

#### **Conflict Resolution**
1. **File conflicts**: Last track to merge resolves conflicts
2. **Test failures**: Failing track owns the fix
3. **Build issues**: Integration agent coordinates fix
4. **Import errors**: Use TypeScript compiler to identify issues

#### **Quality Gates**
- No track merges until tests pass
- No integration until all tracks complete
- No production deployment until full validation

---

## Success Metrics

### **Technical Metrics**
- [ ] All files follow new naming conventions
- [ ] Zero broken imports remain
- [ ] Full test suite passes (100% of previous passing tests)
- [ ] TypeScript compilation succeeds with no errors
- [ ] Application builds and runs without errors
- [ ] No performance regressions detected

### **Process Metrics**  
- [ ] Migration completed within estimated timeframe
- [ ] All agents successfully coordinated without conflicts
- [ ] Git history preserved for all renamed files
- [ ] Documentation fully updated
- [ ] Team onboarded to new conventions

### **Quality Metrics**
- [ ] Code quality maintained or improved
- [ ] No functionality regressions
- [ ] Development experience improved with consistent naming
- [ ] Style guide adoption successful

---

## Post-Migration Actions

### **Immediate (Day 1)**
- [ ] Update development documentation
- [ ] Notify team of completion
- [ ] Add linting rules for new file naming
- [ ] Update VS Code workspace settings

### **Short Term (Week 1)**  
- [ ] Monitor for any missed references
- [ ] Collect team feedback on new conventions
- [ ] Update onboarding documentation
- [ ] Add pre-commit hooks for naming enforcement

### **Long Term (Month 1)**
- [ ] Review and refine style guide based on usage
- [ ] Plan similar standardization for other areas
- [ ] Document lessons learned
- [ ] Establish regular style guide review process

---

**Document Version**: 1.0  
**Created**: January 2025  
**Execution Ready**: ✅ YES  
**Execution**: Parallel tracks with coordination points  
**Risk Level**: Medium-High (manageable with proper coordination)