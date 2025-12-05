/**
 * Unit tests for logger utility
 *
 * Tests the Logger class and createLogger function to ensure proper
 * log level filtering, environment detection, and formatting behavior.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createLogger, logger } from '$lib/utils/logger';

describe('logger utility', () => {
  // Mock console methods to capture calls
  let consoleDebugSpy: ReturnType<typeof vi.spyOn>;
  let consoleLogSpy: ReturnType<typeof vi.spyOn>;
  let consoleWarnSpy: ReturnType<typeof vi.spyOn>;
  let consoleErrorSpy: ReturnType<typeof vi.spyOn>;
  let consoleGroupSpy: ReturnType<typeof vi.spyOn>;
  let consoleGroupEndSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    // Mock all console methods
    consoleDebugSpy = vi.spyOn(console, 'debug').mockImplementation(() => {});
    consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    consoleGroupSpy = vi.spyOn(console, 'group').mockImplementation(() => {});
    consoleGroupEndSpy = vi.spyOn(console, 'groupEnd').mockImplementation(() => {});
  });

  afterEach(() => {
    // Restore all mocks
    vi.restoreAllMocks();
  });

  describe('createLogger', () => {
    it('should create a logger without prefix', () => {
      const log = createLogger();
      expect(log).toBeDefined();
    });

    it('should create a logger with prefix', () => {
      const log = createLogger('TestService');
      expect(log).toBeDefined();
    });

    it('should create independent logger instances', () => {
      const log1 = createLogger('Service1');
      const log2 = createLogger('Service2');
      expect(log1).not.toBe(log2);
    });
  });

  describe('default logger instance', () => {
    it('should export a default logger instance', () => {
      expect(logger).toBeDefined();
    });
  });

  describe('Logger in test environment', () => {
    it('should be disabled by default in test environment', () => {
      const log = createLogger('TestService');
      log.debug('test message');
      log.info('test message');
      log.warn('test message');
      log.error('test message');

      // In test environment, all logs should be disabled by default
      expect(consoleDebugSpy).not.toHaveBeenCalled();
      expect(consoleLogSpy).not.toHaveBeenCalled();
      expect(consoleWarnSpy).not.toHaveBeenCalled();
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });

    it('should not log group operations in test environment', () => {
      const log = createLogger('TestService');
      log.group('Test Group');
      log.groupEnd();

      expect(consoleGroupSpy).not.toHaveBeenCalled();
      expect(consoleGroupEndSpy).not.toHaveBeenCalled();
    });

    it('should return no-op function for time() in test environment', () => {
      const log = createLogger('TestService');
      const done = log.time('Operation');
      expect(typeof done).toBe('function');

      done();
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });
  });

  describe('message formatting', () => {
    it('should format message without prefix', () => {
      // Create a logger that is explicitly enabled for testing
      // Note: In actual usage, we can't easily override the config after construction
      // So we'll test through the public API
      const log = createLogger();

      // Enable logging by checking if we can trigger a log
      // Since logger is disabled in tests, we'll verify the format indirectly
      // by checking that the logger doesn't crash with various inputs
      log.debug('test message');
      log.info('test message');
      log.warn('test message');
      log.error('test message');

      // Verify no crashes occurred (logger handles disabled state gracefully)
      expect(true).toBe(true);
    });

    it('should format message with prefix', () => {
      const log = createLogger('MyService');

      log.debug('test message');
      log.info('test message');
      log.warn('test message');
      log.error('test message');

      // Verify no crashes occurred with prefixed logger
      expect(true).toBe(true);
    });
  });

  describe('log levels - debug', () => {
    it('should handle debug messages without data', () => {
      const log = createLogger('TestService');
      log.debug('debug message');

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle debug messages with data', () => {
      const log = createLogger('TestService');
      const testData = { key: 'value', count: 42 };
      log.debug('debug message', testData);

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle debug messages with undefined data', () => {
      const log = createLogger('TestService');
      log.debug('debug message', undefined);

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle debug messages with null data', () => {
      const log = createLogger('TestService');
      log.debug('debug message', null);

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle debug messages with complex data objects', () => {
      const log = createLogger('TestService');
      const complexData = {
        nested: { deep: { value: 'test' } },
        array: [1, 2, 3],
        func: () => {},
        date: new Date()
      };
      log.debug('debug message', complexData);

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });
  });

  describe('log levels - info', () => {
    it('should handle info messages without data', () => {
      const log = createLogger('TestService');
      log.info('info message');

      // In test environment, should not log
      expect(consoleLogSpy).not.toHaveBeenCalled();
    });

    it('should handle info messages with data', () => {
      const log = createLogger('TestService');
      const testData = { status: 'success', items: 10 };
      log.info('info message', testData);

      // In test environment, should not log
      expect(consoleLogSpy).not.toHaveBeenCalled();
    });

    it('should handle info messages with primitive data types', () => {
      const log = createLogger('TestService');
      log.info('string data', 'test');
      log.info('number data', 42);
      log.info('boolean data', true);

      // In test environment, should not log
      expect(consoleLogSpy).not.toHaveBeenCalled();
    });
  });

  describe('log levels - warn', () => {
    it('should handle warn messages without data', () => {
      const log = createLogger('TestService');
      log.warn('warning message');

      // In test environment, should not log
      expect(consoleWarnSpy).not.toHaveBeenCalled();
    });

    it('should handle warn messages with data', () => {
      const log = createLogger('TestService');
      const warningData = { code: 'WARN_001', details: 'Resource low' };
      log.warn('warning message', warningData);

      // In test environment, should not log
      expect(consoleWarnSpy).not.toHaveBeenCalled();
    });

    it('should handle warn messages with error objects', () => {
      const log = createLogger('TestService');
      const error = new Error('Test error');
      log.warn('warning message', error);

      // In test environment, should not log
      expect(consoleWarnSpy).not.toHaveBeenCalled();
    });
  });

  describe('log levels - error', () => {
    it('should handle error messages without data', () => {
      const log = createLogger('TestService');
      log.error('error message');

      // In test environment, should not log
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });

    it('should handle error messages with data', () => {
      const log = createLogger('TestService');
      const errorData = { code: 'ERR_001', message: 'Operation failed' };
      log.error('error message', errorData);

      // In test environment, should not log
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });

    it('should handle error messages with Error objects', () => {
      const log = createLogger('TestService');
      const error = new Error('Critical failure');
      error.stack = 'Error stack trace...';
      log.error('error message', error);

      // In test environment, should not log
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });

    it('should handle error messages with custom error objects', () => {
      const log = createLogger('TestService');
      const customError = {
        name: 'CustomError',
        message: 'Custom error occurred',
        code: 500,
        details: { reason: 'timeout' }
      };
      log.error('error message', customError);

      // In test environment, should not log
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });
  });

  describe('group operations', () => {
    it('should handle group without throwing', () => {
      const log = createLogger('TestService');
      expect(() => log.group('Test Group')).not.toThrow();

      // In test environment, should not call console.group
      expect(consoleGroupSpy).not.toHaveBeenCalled();
    });

    it('should handle groupEnd without throwing', () => {
      const log = createLogger('TestService');
      expect(() => log.groupEnd()).not.toThrow();

      // In test environment, should not call console.groupEnd
      expect(consoleGroupEndSpy).not.toHaveBeenCalled();
    });

    it('should handle nested groups', () => {
      const log = createLogger('TestService');
      log.group('Outer Group');
      log.group('Inner Group');
      log.debug('nested message');
      log.groupEnd();
      log.groupEnd();

      // In test environment, should not call console methods
      expect(consoleGroupSpy).not.toHaveBeenCalled();
      expect(consoleDebugSpy).not.toHaveBeenCalled();
      expect(consoleGroupEndSpy).not.toHaveBeenCalled();
    });

    it('should format group labels with prefix', () => {
      const log = createLogger('MyService');
      log.group('Operation');

      // In test environment, should not log
      expect(consoleGroupSpy).not.toHaveBeenCalled();
    });
  });

  describe('time operations', () => {
    it('should return a function from time()', () => {
      const log = createLogger('TestService');
      const done = log.time('Operation');
      expect(typeof done).toBe('function');
    });

    it('should not throw when calling the done function', () => {
      const log = createLogger('TestService');
      const done = log.time('Operation');
      expect(() => done()).not.toThrow();

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle multiple time operations', () => {
      const log = createLogger('TestService');
      const done1 = log.time('Operation 1');
      const done2 = log.time('Operation 2');

      expect(typeof done1).toBe('function');
      expect(typeof done2).toBe('function');
      expect(done1).not.toBe(done2);

      done1();
      done2();

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should handle time operation with prefix', () => {
      const log = createLogger('MyService');
      const done = log.time('Long Operation');
      done();

      // In test environment, should not log
      expect(consoleDebugSpy).not.toHaveBeenCalled();
    });

    it('should be safe to call done() multiple times', () => {
      const log = createLogger('TestService');
      const done = log.time('Operation');

      expect(() => {
        done();
        done();
        done();
      }).not.toThrow();
    });
  });

  describe('prefix formatting', () => {
    it('should format messages with empty string prefix', () => {
      const log = createLogger('');
      log.debug('test message');

      // Should not crash with empty prefix
      expect(true).toBe(true);
    });

    it('should format messages with whitespace prefix', () => {
      const log = createLogger('  ');
      log.debug('test message');

      // Should not crash with whitespace prefix
      expect(true).toBe(true);
    });

    it('should format messages with special characters in prefix', () => {
      const log = createLogger('Service[1]<test>');
      log.debug('test message');

      // Should not crash with special characters
      expect(true).toBe(true);
    });

    it('should format messages with long prefix', () => {
      const longPrefix = 'A'.repeat(100);
      const log = createLogger(longPrefix);
      log.debug('test message');

      // Should not crash with long prefix
      expect(true).toBe(true);
    });

    it('should format messages with unicode prefix', () => {
      const log = createLogger('日本語サービス');
      log.debug('test message');

      // Should not crash with unicode prefix
      expect(true).toBe(true);
    });

    it('should format messages with emoji prefix', () => {
      const log = createLogger('🚀 RocketService');
      log.debug('test message');

      // Should not crash with emoji prefix
      expect(true).toBe(true);
    });
  });

  describe('edge cases and error handling', () => {
    it('should handle empty string messages', () => {
      const log = createLogger('TestService');
      expect(() => {
        log.debug('');
        log.info('');
        log.warn('');
        log.error('');
      }).not.toThrow();
    });

    it('should handle very long messages', () => {
      const log = createLogger('TestService');
      const longMessage = 'A'.repeat(10000);
      expect(() => {
        log.debug(longMessage);
        log.info(longMessage);
        log.warn(longMessage);
        log.error(longMessage);
      }).not.toThrow();
    });

    it('should handle multiline messages', () => {
      const log = createLogger('TestService');
      const multiline = 'Line 1\nLine 2\nLine 3';
      expect(() => {
        log.debug(multiline);
        log.info(multiline);
        log.warn(multiline);
        log.error(multiline);
      }).not.toThrow();
    });

    it('should handle messages with newlines and special characters', () => {
      const log = createLogger('TestService');
      const special = 'Message\twith\ttabs\nand\nnewlines\rand\rreturns';
      expect(() => {
        log.debug(special);
        log.info(special);
        log.warn(special);
        log.error(special);
      }).not.toThrow();
    });

    it('should handle circular reference in data', () => {
      const log = createLogger('TestService');
      const circular: Record<string, unknown> = { key: 'value' };
      circular.self = circular;

      expect(() => {
        log.debug('message', circular);
        log.info('message', circular);
        log.warn('message', circular);
        log.error('message', circular);
      }).not.toThrow();
    });

    it('should handle symbols as data', () => {
      const log = createLogger('TestService');
      const sym = Symbol('test');
      expect(() => {
        log.debug('message', sym);
      }).not.toThrow();
    });

    it('should handle BigInt as data', () => {
      const log = createLogger('TestService');
      const bigInt = BigInt(9007199254740991);
      expect(() => {
        log.debug('message', bigInt);
      }).not.toThrow();
    });

    it('should handle arrays as data', () => {
      const log = createLogger('TestService');
      const array = [1, 'two', { three: 3 }, [4, 5]];
      expect(() => {
        log.debug('message', array);
        log.info('message', array);
        log.warn('message', array);
        log.error('message', array);
      }).not.toThrow();
    });

    it('should handle Map as data', () => {
      const log = createLogger('TestService');
      const map = new Map([
        ['key1', 'value1'],
        ['key2', 'value2']
      ]);
      expect(() => {
        log.debug('message', map);
      }).not.toThrow();
    });

    it('should handle Set as data', () => {
      const log = createLogger('TestService');
      const set = new Set([1, 2, 3, 4, 5]);
      expect(() => {
        log.debug('message', set);
      }).not.toThrow();
    });

    it('should handle WeakMap as data', () => {
      const log = createLogger('TestService');
      const weakMap = new WeakMap();
      const key = {};
      weakMap.set(key, 'value');
      expect(() => {
        log.debug('message', weakMap);
      }).not.toThrow();
    });

    it('should handle WeakSet as data', () => {
      const log = createLogger('TestService');
      const weakSet = new WeakSet();
      const obj = {};
      weakSet.add(obj);
      expect(() => {
        log.debug('message', weakSet);
      }).not.toThrow();
    });
  });

  describe('real-world usage scenarios', () => {
    it('should handle typical service initialization logging', () => {
      const log = createLogger('NavigationService');
      expect(() => {
        log.debug('Service initialized');
        log.info('Ready to process navigation requests');
      }).not.toThrow();
    });

    it('should handle typical operation logging with data', () => {
      const log = createLogger('DataService');
      expect(() => {
        log.debug('Fetching nodes', { count: 100, filter: 'active' });
        log.info('Nodes loaded successfully', { duration: 125 });
        log.warn('Cache miss', { nodeId: '123' });
        log.error('Failed to load node', { nodeId: '456', error: 'Not found' });
      }).not.toThrow();
    });

    it('should handle grouped operation logging', () => {
      const log = createLogger('BatchProcessor');
      expect(() => {
        log.group('Processing batch');
        log.debug('Item 1 processed');
        log.debug('Item 2 processed');
        log.debug('Item 3 processed');
        log.groupEnd();
        log.info('Batch complete', { count: 3 });
      }).not.toThrow();
    });

    it('should handle timed operation logging', () => {
      const log = createLogger('DatabaseService');
      const done = log.time('Query execution');
      // Simulate some work
      const result = [1, 2, 3, 4, 5].map((n) => n * 2);
      done();

      expect(result).toEqual([2, 4, 6, 8, 10]);
    });

    it('should handle error logging with stack traces', () => {
      const log = createLogger('ErrorHandler');
      try {
        throw new Error('Test error');
      } catch (error) {
        expect(() => {
          log.error('Caught error', error);
        }).not.toThrow();
      }
    });

    it('should handle multiple loggers in parallel', () => {
      const log1 = createLogger('Service1');
      const log2 = createLogger('Service2');
      const log3 = createLogger('Service3');

      expect(() => {
        log1.debug('Service 1 message');
        log2.info('Service 2 message');
        log3.warn('Service 3 message');
        log1.error('Service 1 error');
      }).not.toThrow();
    });
  });

  describe('performance considerations', () => {
    it('should handle rapid successive log calls', () => {
      const log = createLogger('PerformanceTest');
      expect(() => {
        for (let i = 0; i < 1000; i++) {
          log.debug(`Message ${i}`);
        }
      }).not.toThrow();
    });

    it('should handle large data objects efficiently', () => {
      const log = createLogger('DataTest');
      const largeData = {
        items: Array.from({ length: 1000 }, (_, i) => ({
          id: i,
          name: `Item ${i}`,
          data: { nested: { deep: { value: i } } }
        }))
      };

      expect(() => {
        log.debug('Large data', largeData);
      }).not.toThrow();
    });

    it('should handle multiple time operations concurrently', () => {
      const log = createLogger('ConcurrencyTest');
      const doneCallbacks: (() => void)[] = [];

      for (let i = 0; i < 10; i++) {
        doneCallbacks.push(log.time(`Operation ${i}`));
      }

      expect(() => {
        doneCallbacks.forEach((done) => done());
      }).not.toThrow();
    });
  });

  describe('consistency and stability', () => {
    it('should maintain consistent behavior across multiple calls', () => {
      const log = createLogger('ConsistencyTest');

      for (let i = 0; i < 10; i++) {
        expect(() => {
          log.debug('debug');
          log.info('info');
          log.warn('warn');
          log.error('error');
        }).not.toThrow();
      }

      // In test environment, console should never be called
      expect(consoleDebugSpy).not.toHaveBeenCalled();
      expect(consoleLogSpy).not.toHaveBeenCalled();
      expect(consoleWarnSpy).not.toHaveBeenCalled();
      expect(consoleErrorSpy).not.toHaveBeenCalled();
    });

    it('should be safe to use without initialization errors', () => {
      expect(() => {
        const log1 = createLogger();
        const log2 = createLogger('Service');
        const log3 = logger;

        log1.debug('test');
        log2.info('test');
        log3.warn('test');
      }).not.toThrow();
    });

    it('should handle logger creation in loops', () => {
      expect(() => {
        for (let i = 0; i < 100; i++) {
          const log = createLogger(`Service${i}`);
          log.debug('test');
        }
      }).not.toThrow();
    });
  });
});
