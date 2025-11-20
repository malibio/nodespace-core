/**
 * Test-aware logger utility
 * Automatically silences logs during test execution to improve performance
 */

type LogLevel = 'debug' | 'info' | 'warn' | 'error';

interface LoggerConfig {
  enabled: boolean;
  level: LogLevel;
  prefix?: string;
}

class Logger {
  private config: LoggerConfig;

  constructor(config?: Partial<LoggerConfig>) {
    // Disable logging in test environment by default
    const isTest = typeof import.meta !== 'undefined' && import.meta.env?.VITEST === 'true';
    const isDebug = typeof import.meta !== 'undefined' && import.meta.env?.DEV === true;

    this.config = {
      enabled: !isTest || isDebug,
      level: 'info',
      prefix: '',
      ...config
    };
  }

  private shouldLog(level: LogLevel): boolean {
    if (!this.config.enabled) return false;

    const levels: LogLevel[] = ['debug', 'info', 'warn', 'error'];
    const currentLevelIndex = levels.indexOf(this.config.level);
    const requestedLevelIndex = levels.indexOf(level);

    return requestedLevelIndex >= currentLevelIndex;
  }

  private formatMessage(message: string): string {
    const prefix = this.config.prefix ? `[${this.config.prefix}] ` : '';
    return `${prefix}${message}`;
  }

  debug(message: string, data?: unknown): void {
    if (this.shouldLog('debug')) {
      if (data !== undefined) {
        console.debug(this.formatMessage(message), data);
      } else {
        console.debug(this.formatMessage(message));
      }
    }
  }

  info(message: string, data?: unknown): void {
    if (this.shouldLog('info')) {
      if (data !== undefined) {
        console.log(this.formatMessage(message), data);
      } else {
        console.log(this.formatMessage(message));
      }
    }
  }

  warn(message: string, data?: unknown): void {
    if (this.shouldLog('warn')) {
      if (data !== undefined) {
        console.warn(this.formatMessage(message), data);
      } else {
        console.warn(this.formatMessage(message));
      }
    }
  }

  error(message: string, data?: unknown): void {
    if (this.shouldLog('error')) {
      if (data !== undefined) {
        console.error(this.formatMessage(message), data);
      } else {
        console.error(this.formatMessage(message));
      }
    }
  }
}

/**
 * Create a logger instance with optional prefix
 * Logs are automatically disabled during tests for better performance
 */
export function createLogger(prefix?: string): Logger {
  return new Logger({ prefix });
}

/**
 * Default logger instance
 */
export const logger = new Logger();
