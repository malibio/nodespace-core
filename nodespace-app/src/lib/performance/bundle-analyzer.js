/**
 * Bundle Size Analysis for CodeMirror Integration
 * 
 * This utility provides insights into CodeMirror's bundle impact
 * and identifies opportunities for tree shaking optimization.
 */

export class BundleAnalyzer {
  constructor() {
    this.analysis = null;
  }

  /**
   * Analyze CodeMirror bundle impact
   */
  async analyzeBundleImpact() {
    const analysis = {
      timestamp: new Date().toISOString(),
      packages: {},
      totalSize: 0,
      compressionRatio: 0,
      recommendations: []
    };

    // Analyze individual CodeMirror packages
    analysis.packages = await this.analyzeCodeMirrorPackages();
    
    // Calculate total impact
    analysis.totalSize = Object.values(analysis.packages).reduce((sum, pkg) => sum + pkg.estimatedSize, 0);
    
    // Generate recommendations
    analysis.recommendations = this.generateBundleRecommendations(analysis.packages);
    
    this.analysis = analysis;
    return analysis;
  }

  /**
   * Analyze individual CodeMirror packages
   */
  async analyzeCodeMirrorPackages() {
    const packages = {};

    try {
      // @codemirror/state analysis
      packages['@codemirror/state'] = {
        version: '6.5.2',
        features: [
          'EditorState',
          'StateEffect',
          'StateField',
          'Transaction',
          'Selection',
          'Text',
          'ChangeSet',
          'Compartment'
        ],
        usedFeatures: [
          'EditorState',
          'Transaction' // from CodeMirrorEditor.svelte
        ],
        unusedFeatures: [
          'StateEffect',
          'StateField',
          'Selection',
          'ChangeSet',
          'Compartment'
        ],
        estimatedSize: 45000, // bytes (estimated)
        treeshakingPotential: 'high',
        notes: 'Core state management - minimal usage detected'
      };

      // @codemirror/view analysis
      packages['@codemirror/view'] = {
        version: '6.38.1',
        features: [
          'EditorView',
          'ViewUpdate',
          'Decoration',
          'DecorationSet',
          'WidgetType',
          'BlockInfo',
          'ViewPlugin',
          'keymap',
          'drawSelection',
          'dropCursor',
          'highlightSelectionMatches'
        ],
        usedFeatures: [
          'EditorView',
          'ViewUpdate' // from CodeMirrorEditor.svelte
        ],
        unusedFeatures: [
          'Decoration',
          'DecorationSet',
          'WidgetType',
          'BlockInfo',
          'ViewPlugin',
          'keymap',
          'drawSelection',
          'dropCursor',
          'highlightSelectionMatches'
        ],
        estimatedSize: 120000, // bytes (estimated)
        treeshakingPotential: 'very high',
        notes: 'Large package with many unused features for basic editing'
      };

      // @codemirror/lang-markdown analysis
      packages['@codemirror/lang-markdown'] = {
        version: '6.3.4',
        features: [
          'markdown',
          'markdownLanguage',
          'markdownKeymap',
          'insertNewlineContinueMarkup',
          'deleteMarkupBackward'
        ],
        usedFeatures: [
          'markdown' // from CodeMirrorEditor.svelte
        ],
        unusedFeatures: [
          'markdownKeymap',
          'insertNewlineContinueMarkup',
          'deleteMarkupBackward'
        ],
        estimatedSize: 35000, // bytes (estimated)
        treeshakingPotential: 'medium',
        notes: 'Markdown support - some unused features'
      };

      // @codemirror/commands analysis
      packages['@codemirror/commands'] = {
        version: '6.8.1',
        features: [
          'history',
          'historyKeymap',
          'defaultKeymap',
          'indentWithTab',
          'undo',
          'redo',
          'selectAll',
          'cursorDocStart',
          'cursorDocEnd',
          'selectLine',
          'selectParentSyntax',
          'simplifySelection',
          'copyLineUp',
          'copyLineDown',
          'deleteLine',
          'moveLineUp',
          'moveLineDown'
        ],
        usedFeatures: [
          // None directly used in our current implementation
        ],
        unusedFeatures: [
          'history',
          'historyKeymap',
          'defaultKeymap',
          'indentWithTab',
          'undo',
          'redo',
          'selectAll',
          'cursorDocStart',
          'cursorDocEnd',
          'selectLine',
          'selectParentSyntax',
          'simplifySelection',
          'copyLineUp',
          'copyLineDown',
          'deleteLine',
          'moveLineUp',
          'moveLineDown'
        ],
        estimatedSize: 55000, // bytes (estimated)
        treeshakingPotential: 'extremely high',
        notes: 'Commands package completely unused - can be removed'
      };

    } catch (error) {
      console.warn('Error analyzing packages:', error);
    }

    return packages;
  }

  /**
   * Generate bundle optimization recommendations
   */
  generateBundleRecommendations(packages) {
    const recommendations = [];

    Object.entries(packages).forEach(([packageName, info]) => {
      switch (info.treeshakingPotential) {
        case 'extremely high':
          recommendations.push({
            priority: 'critical',
            package: packageName,
            action: 'remove',
            impact: `~${(info.estimatedSize / 1024).toFixed(1)}KB savings`,
            description: `Package is completely unused and can be removed`,
            implementation: `Remove '${packageName}' from package.json dependencies`
          });
          break;

        case 'very high':
          recommendations.push({
            priority: 'high',
            package: packageName,
            action: 'tree-shake',
            impact: `~${((info.estimatedSize * 0.6) / 1024).toFixed(1)}KB potential savings`,
            description: `Only ${info.usedFeatures.length}/${info.features.length} features used`,
            implementation: `Import specific features instead of full package`
          });
          break;

        case 'high':
          recommendations.push({
            priority: 'medium',
            package: packageName,
            action: 'optimize',
            impact: `~${((info.estimatedSize * 0.3) / 1024).toFixed(1)}KB potential savings`,
            description: `Several unused features detected`,
            implementation: `Review usage patterns and import only needed features`
          });
          break;

        case 'medium':
          recommendations.push({
            priority: 'low',
            package: packageName,
            action: 'review',
            impact: `~${((info.estimatedSize * 0.15) / 1024).toFixed(1)}KB potential savings`,
            description: `Some optimization opportunities available`,
            implementation: `Consider selective imports for unused features`
          });
          break;
      }
    });

    // Sort by priority
    const priorityOrder = { critical: 0, high: 1, medium: 2, low: 3 };
    recommendations.sort((a, b) => priorityOrder[a.priority] - priorityOrder[b.priority]);

    return recommendations;
  }

  /**
   * Calculate potential savings from all recommendations
   */
  calculatePotentialSavings() {
    if (!this.analysis) return 0;

    return this.analysis.recommendations.reduce((total, rec) => {
      const match = rec.impact.match(/~(\d+(?:\.\d+)?)KB/);
      return total + (match ? parseFloat(match[1]) : 0);
    }, 0);
  }

  /**
   * Generate tree shaking implementation code
   */
  generateTreeShakingCode() {
    if (!this.analysis) return null;

    const optimizations = {};

    // Generate optimized imports for each package
    Object.entries(this.analysis.packages).forEach(([packageName, info]) => {
      if (info.usedFeatures.length > 0) {
        optimizations[packageName] = {
          current: `import { ${info.features.join(', ')} } from '${packageName}';`,
          optimized: `import { ${info.usedFeatures.join(', ')} } from '${packageName}';`,
          description: `Import only used features from ${packageName}`
        };
      } else if (info.treeshakingPotential === 'extremely high') {
        optimizations[packageName] = {
          current: `import {...} from '${packageName}';`,
          optimized: `// Remove ${packageName} - completely unused`,
          description: `Remove unused package ${packageName}`
        };
      }
    });

    return optimizations;
  }

  /**
   * Export analysis results
   */
  exportResults() {
    if (!this.analysis) return null;

    return {
      ...this.analysis,
      potentialSavings: this.calculatePotentialSavings(),
      treeShakingCode: this.generateTreeShakingCode(),
      summary: {
        totalPackages: Object.keys(this.analysis.packages).length,
        totalCurrentSize: (this.analysis.totalSize / 1024).toFixed(1) + 'KB',
        totalRecommendations: this.analysis.recommendations.length,
        criticalRecommendations: this.analysis.recommendations.filter(r => r.priority === 'critical').length,
        implementationComplexity: this.assessImplementationComplexity()
      }
    };
  }

  /**
   * Assess implementation complexity
   */
  assessImplementationComplexity() {
    if (!this.analysis) return 'unknown';

    const critical = this.analysis.recommendations.filter(r => r.priority === 'critical').length;
    const high = this.analysis.recommendations.filter(r => r.priority === 'high').length;

    if (critical > 2 || high > 3) return 'high';
    if (critical > 0 || high > 1) return 'medium';
    return 'low';
  }
}

/**
 * Create a simple bundle size comparison
 */
export function createBundleSizeComparison(beforeSize, afterSize, target) {
  const difference = beforeSize - afterSize;
  const percentReduction = ((difference / beforeSize) * 100).toFixed(1);
  const targetMet = afterSize <= target;

  return {
    before: beforeSize,
    after: afterSize,
    difference,
    percentReduction,
    target,
    targetMet,
    status: targetMet ? 'success' : 'needs-improvement',
    message: targetMet 
      ? `Bundle size optimized: ${percentReduction}% reduction`
      : `Bundle size still ${(afterSize - target).toFixed(0)}KB over target`
  };
}