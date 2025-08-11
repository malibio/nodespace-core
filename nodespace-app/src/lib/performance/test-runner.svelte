<!--
  Performance Test Runner Component
  
  Provides UI for running performance benchmarks and displaying results
-->

<script lang="ts">
  import { onMount } from 'svelte';
  import { EditorView } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { markdown } from '@codemirror/lang-markdown';
  import { PerformanceBenchmarks } from './benchmarks.js';
  
  let benchmarks: PerformanceBenchmarks;
  let results: any = null;
  let isRunning = false;
  let currentTest = '';
  let testContainer: HTMLDivElement;

  onMount(() => {
    benchmarks = new PerformanceBenchmarks();
  });

  async function runBenchmarks() {
    if (!benchmarks) return;
    
    isRunning = true;
    results = null;
    currentTest = 'Initializing...';

    try {
      // Create editor factory for initialization tests
      const editorFactory = async () => {
        const state = EditorState.create({
          doc: '',
          extensions: [
            EditorView.theme({
              '&': {
                fontSize: '14px',
                lineHeight: '1.4',
                fontFamily: 'inherit'
              },
              '.cm-content': {
                padding: '0',
                minHeight: '20px',
                caretColor: 'hsl(var(--foreground))',
                color: 'hsl(var(--foreground))'
              }
            }),
            markdown()
          ]
        });

        const view = new EditorView({
          state,
          parent: testContainer
        });

        return view;
      };

      // Create a test editor instance
      const testEditor = await editorFactory();
      
      // Run benchmarks
      results = await benchmarks.runAllBenchmarks(testEditor, editorFactory);
      
      // Cleanup test editor
      testEditor.destroy();
      
    } catch (error) {
      console.error('Benchmark error:', error);
      results = { error: error.message };
    } finally {
      isRunning = false;
      currentTest = '';
    }
  }

  function formatTime(ms: number): string {
    if (ms < 1) return `${(ms * 1000).toFixed(2)}μs`;
    return `${ms.toFixed(2)}ms`;
  }

  function formatMemory(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    
    return `${size.toFixed(2)} ${units[unitIndex]}`;
  }

  function getStatusColor(passed: boolean): string {
    return passed ? 'text-green-600' : 'text-red-600';
  }

  function getRecommendationColor(priority: string): string {
    switch (priority) {
      case 'critical': return 'text-red-600 bg-red-50';
      case 'high': return 'text-orange-600 bg-orange-50';
      case 'medium': return 'text-yellow-600 bg-yellow-50';
      default: return 'text-blue-600 bg-blue-50';
    }
  }
</script>

<div class="performance-test-runner">
  <div class="header">
    <h1>NodeSpace Performance Benchmarks</h1>
    <p>Testing CodeMirror integration performance</p>
  </div>

  <div class="controls">
    <button 
      class="run-button" 
      class:running={isRunning}
      disabled={isRunning}
      on:click={runBenchmarks}
    >
      {#if isRunning}
        Running Tests... {currentTest}
      {:else}
        Run Performance Benchmarks
      {/if}
    </button>
  </div>

  <!-- Hidden test container -->
  <div bind:this={testContainer} style="position: absolute; left: -9999px; top: -9999px;"></div>

  {#if results}
    <div class="results">
      {#if results.error}
        <div class="error">
          <h2>Error</h2>
          <p>{results.error}</p>
        </div>
      {:else}
        <!-- Summary -->
        <div class="summary">
          <h2>Summary</h2>
          <div class="summary-stats">
            <div class="stat">
              <div class="stat-label">Overall Status</div>
              <div class="stat-value {getStatusColor(results.summary.allTestsPassed)}">
                {results.summary.allTestsPassed ? '✅ PASSED' : '❌ FAILED'}
              </div>
            </div>
            <div class="stat">
              <div class="stat-label">Tests Passed</div>
              <div class="stat-value">{results.summary.passedTests}/{results.summary.totalTests}</div>
            </div>
            <div class="stat">
              <div class="stat-label">Critical Failures</div>
              <div class="stat-value {getStatusColor(results.summary.criticalFailures === 0)}">
                {results.summary.criticalFailures}
              </div>
            </div>
          </div>
        </div>

        <!-- Test Results -->
        <div class="test-results">
          <h2>Test Results</h2>
          
          {#if results.tests.editorInit}
            <div class="test-result">
              <h3>Editor Initialization</h3>
              <div class="test-stats">
                <div class="stat">
                  <span class="stat-label">Status:</span>
                  <span class="{getStatusColor(results.tests.editorInit.passed)}">
                    {results.tests.editorInit.passed ? 'PASSED' : 'FAILED'}
                  </span>
                </div>
                <div class="stat">
                  <span class="stat-label">Average:</span>
                  <span>{formatTime(results.tests.editorInit.average)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">Target:</span>
                  <span>{formatTime(results.tests.editorInit.target)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">Range:</span>
                  <span>{formatTime(results.tests.editorInit.min)} - {formatTime(results.tests.editorInit.max)}</span>
                </div>
              </div>
            </div>
          {/if}

          {#if results.tests.keystroke}
            <div class="test-result">
              <h3>Keystroke Response</h3>
              <div class="test-stats">
                <div class="stat">
                  <span class="stat-label">Status:</span>
                  <span class="{getStatusColor(results.tests.keystroke.passed)}">
                    {results.tests.keystroke.passed ? 'PASSED' : 'FAILED'}
                  </span>
                </div>
                <div class="stat">
                  <span class="stat-label">Average:</span>
                  <span>{formatTime(results.tests.keystroke.average)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">P95:</span>
                  <span>{formatTime(results.tests.keystroke.p95)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">P99:</span>
                  <span>{formatTime(results.tests.keystroke.p99)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">Target:</span>
                  <span>{formatTime(results.tests.keystroke.target)}</span>
                </div>
              </div>
            </div>
          {/if}

          {#if results.tests.largeDoc}
            <div class="test-result">
              <h3>Large Document Handling</h3>
              <div class="test-stats">
                <div class="stat">
                  <span class="stat-label">Status:</span>
                  <span class="{getStatusColor(results.tests.largeDoc.passed)}">
                    {results.tests.largeDoc.passed ? 'PASSED' : 'FAILED'}
                  </span>
                </div>
              </div>
              <div class="large-doc-results">
                {#each results.tests.largeDoc.results as docResult}
                  <div class="doc-size-result">
                    <strong>{formatMemory(docResult.size)} document:</strong>
                    Load: {formatTime(docResult.loadTime)} 
                    ({docResult.loadPassed ? '✅' : '❌'})
                    | Scroll: {formatTime(docResult.scrollTime)} 
                    ({docResult.scrollPassed ? '✅' : '❌'})
                  </div>
                {/each}
              </div>
            </div>
          {/if}

          {#if results.tests.memoryLeak}
            <div class="test-result">
              <h3>Memory Usage</h3>
              <div class="test-stats">
                <div class="stat">
                  <span class="stat-label">Status:</span>
                  <span class="{getStatusColor(results.tests.memoryLeak.passed)}">
                    {results.tests.memoryLeak.passed ? 'PASSED' : 'FAILED'}
                  </span>
                </div>
                <div class="stat">
                  <span class="stat-label">Growth per Operation:</span>
                  <span>{formatMemory(results.tests.memoryLeak.memoryGrowth.avgGrowthPerOperation)}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">Total Operations:</span>
                  <span>{results.tests.memoryLeak.operations}</span>
                </div>
              </div>
            </div>
          {/if}
        </div>

        <!-- Recommendations -->
        {#if results.summary.recommendations && results.summary.recommendations.length > 0}
          <div class="recommendations">
            <h2>Optimization Recommendations</h2>
            {#each results.summary.recommendations as rec}
              <div class="recommendation {getRecommendationColor(rec.priority)}">
                <div class="rec-header">
                  <span class="rec-priority">{rec.priority.toUpperCase()}</span>
                  <span class="rec-area">{rec.area}</span>
                </div>
                <div class="rec-issue">{rec.issue}</div>
                <ul class="rec-suggestions">
                  {#each rec.suggestions as suggestion}
                    <li>{suggestion}</li>
                  {/each}
                </ul>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Raw Data (collapsed by default) -->
        <details class="raw-data">
          <summary>Raw Test Data</summary>
          <pre>{JSON.stringify(results, null, 2)}</pre>
        </details>
      {/if}
    </div>
  {/if}
</div>

<style>
  .performance-test-runner {
    max-width: 1200px;
    margin: 0 auto;
    padding: 20px;
    font-family: system-ui, -apple-system, sans-serif;
  }

  .header {
    text-align: center;
    margin-bottom: 30px;
  }

  .header h1 {
    font-size: 2rem;
    font-weight: 600;
    margin-bottom: 8px;
  }

  .header p {
    color: #666;
    font-size: 1.1rem;
  }

  .controls {
    text-align: center;
    margin-bottom: 30px;
  }

  .run-button {
    background: #007acc;
    color: white;
    border: none;
    padding: 12px 24px;
    border-radius: 8px;
    font-size: 1rem;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .run-button:hover:not(:disabled) {
    background: #005999;
  }

  .run-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .run-button.running {
    background: #666;
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: 30px;
  }

  .summary {
    background: #f8f9fa;
    padding: 20px;
    border-radius: 8px;
    border: 1px solid #e9ecef;
  }

  .summary h2 {
    margin-top: 0;
    margin-bottom: 16px;
    font-size: 1.5rem;
    font-weight: 600;
  }

  .summary-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .stat-label {
    font-size: 0.9rem;
    color: #666;
    font-weight: 500;
  }

  .stat-value {
    font-size: 1.2rem;
    font-weight: 600;
  }

  .test-results h2 {
    margin-bottom: 20px;
    font-size: 1.5rem;
    font-weight: 600;
  }

  .test-result {
    background: white;
    border: 1px solid #e9ecef;
    border-radius: 8px;
    padding: 20px;
    margin-bottom: 16px;
  }

  .test-result h3 {
    margin-top: 0;
    margin-bottom: 16px;
    font-size: 1.2rem;
    font-weight: 600;
  }

  .test-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    margin-bottom: 16px;
  }

  .test-stats .stat {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .large-doc-results {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .doc-size-result {
    padding: 8px;
    background: #f8f9fa;
    border-radius: 4px;
    font-family: monospace;
    font-size: 0.9rem;
  }

  .recommendations h2 {
    margin-bottom: 20px;
    font-size: 1.5rem;
    font-weight: 600;
  }

  .recommendation {
    padding: 16px;
    border-radius: 8px;
    border: 1px solid currentColor;
    margin-bottom: 16px;
  }

  .rec-header {
    display: flex;
    gap: 12px;
    margin-bottom: 8px;
    font-weight: 600;
  }

  .rec-priority {
    font-size: 0.8rem;
    padding: 2px 6px;
    border-radius: 4px;
    background: currentColor;
    color: white;
  }

  .rec-area {
    text-transform: capitalize;
  }

  .rec-issue {
    margin-bottom: 12px;
    font-weight: 500;
  }

  .rec-suggestions {
    margin: 0;
    padding-left: 20px;
  }

  .rec-suggestions li {
    margin-bottom: 4px;
  }

  .error {
    background: #fff5f5;
    color: #c53030;
    padding: 20px;
    border-radius: 8px;
    border: 1px solid #feb2b2;
  }

  .error h2 {
    margin-top: 0;
    margin-bottom: 12px;
  }

  .raw-data {
    border: 1px solid #e9ecef;
    border-radius: 8px;
    padding: 16px;
  }

  .raw-data summary {
    cursor: pointer;
    font-weight: 600;
    margin-bottom: 16px;
  }

  .raw-data pre {
    background: #f8f9fa;
    padding: 16px;
    border-radius: 4px;
    overflow-x: auto;
    font-size: 0.9rem;
    margin: 0;
  }
</style>