import { setupServer } from 'msw/node';
import { handlers } from './handlers';

// Setup mock server for Node.js environment (testing)
export const server = setupServer(...handlers);

// Browser setup would be different (for development/demo mode)
export { handlers };