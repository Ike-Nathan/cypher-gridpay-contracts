import test from 'node:test';
import assert from 'node:assert/strict';
import {
  decodeGridPayError,
  extractErrorCode,
  isContractError,
  isRetryableError,
  formatDiagnosticError,
  PAYMENT_ERRORS,
  ESCROW_ERRORS,
  REFUND_ERRORS,
  ADMIN_ERRORS,
} from '../dist/index.js';

test('Error Maps Completeness', (t) => {
  assert.ok(PAYMENT_ERRORS.size > 50, `Payment errors should be populated (got ${PAYMENT_ERRORS.size})`);
  assert.ok(ESCROW_ERRORS.size > 20, `Escrow errors should be populated (got ${ESCROW_ERRORS.size})`);
  assert.ok(REFUND_ERRORS.size > 20, `Refund errors should be populated (got ${REFUND_ERRORS.size})`);
  assert.ok(ADMIN_ERRORS.size >= 3, `Admin errors should be populated (got ${ADMIN_ERRORS.size})`);
});

test('extractErrorCode handles multiple error representations', (t) => {
  // Pure numbers
  assert.equal(extractErrorCode(106), 106);
  assert.equal(extractErrorCode('106'), 106);

  // Soroban HostError / Contract formats
  assert.equal(extractErrorCode('HostError: Error(Contract, #106)'), 106);
  assert.equal(extractErrorCode('Error(Contract, 106)'), 106);
  assert.equal(extractErrorCode('Transaction failed with contract error: 106'), 106);

  // Objects
  assert.equal(extractErrorCode({ code: 106 }), 106);
  assert.equal(extractErrorCode({ error: 'Error(Contract, #106)' }), 106);
  assert.equal(extractErrorCode({ message: 'HostError: Error(Contract, #106)' }), 106);
  assert.equal(extractErrorCode({ result: { error: 'Error(Contract, #106)' } }), 106);

  // Non-errors
  assert.equal(extractErrorCode('Regular network timeout'), null);
  assert.equal(extractErrorCode({}), null);
  assert.equal(extractErrorCode(null), null);
  assert.equal(extractErrorCode(undefined), null);
});

test('decodeGridPayError decodes payment errors accurately', (t) => {
  // Payment BasicError: RateLimitExceeded (106)
  const decoded = decodeGridPayError('HostError: Error(Contract, #106)');
  assert.ok(decoded);
  assert.equal(decoded.contract, 'payment');
  assert.equal(decoded.code, 106);
  assert.equal(decoded.name, 'RateLimitExceeded');
  assert.equal(decoded.retryable, true);
  assert.ok(decoded.resolution.includes('rate limit'));
});

test('decodeGridPayError decodes escrow errors with hint', (t) => {
  // Escrow NotFound (200)
  const decoded = decodeGridPayError(200, 'escrow');
  assert.ok(decoded);
  assert.equal(decoded.contract, 'escrow');
  assert.equal(decoded.code, 200);
  assert.equal(decoded.name, 'NotFound');
});

test('decodeGridPayError decodes refund errors', (t) => {
  // Refund CircuitBreakerTripped (29)
  const decoded = decodeGridPayError(29, 'refund');
  assert.ok(decoded);
  assert.equal(decoded.contract, 'refund');
  assert.equal(decoded.code, 29);
  assert.equal(decoded.name, 'CircuitBreakerTripped');
  assert.equal(decoded.retryable, true);
});

test('decodeGridPayError decodes admin errors', (t) => {
  // Admin Unauthorized (3)
  const decoded = decodeGridPayError(3, 'admin');
  assert.ok(decoded);
  assert.equal(decoded.contract, 'admin');
  assert.equal(decoded.code, 3);
  assert.equal(decoded.name, 'Unauthorized');
  assert.equal(decoded.retryable, false);
});

test('isContractError & isRetryableError helpers', (t) => {
  assert.equal(isContractError('Error(Contract, #106)'), true);
  assert.equal(isContractError('Network disconnected'), false);

  assert.equal(isRetryableError('Error(Contract, #106)'), true); // RateLimitExceeded
  assert.equal(isRetryableError('Error(Contract, #100)'), false); // Unauthorized
});

test('formatDiagnosticError outputs clean actionable diagnostics', (t) => {
  const formatted = formatDiagnosticError('Error(Contract, #106)');
  assert.ok(formatted.includes('[PAYMENT::RateLimitExceeded (#106)]'));
  assert.ok(formatted.includes('Remediation:'));

  const rawFallback = formatDiagnosticError('Standard HTTP 500');
  assert.equal(rawFallback, 'Standard HTTP 500');
});
