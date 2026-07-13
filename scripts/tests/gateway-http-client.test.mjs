import assert from 'node:assert/strict';
import http from 'node:http';
import https from 'node:https';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const { selectGatewayHttpClient } = require('../gateway-http-client.js');

test('selects the Node HTTP client for a local gateway URL', () => {
  const selected = selectGatewayHttpClient('http://127.0.0.1:8080/api');

  assert.equal(selected.client, http);
  assert.equal(selected.url.protocol, 'http:');
  assert.equal(selected.url.port, '8080');
});

test('selects the Node HTTPS client for a TLS gateway URL', () => {
  const selected = selectGatewayHttpClient(new URL('https://gateway.local:8443/api'));

  assert.equal(selected.client, https);
  assert.equal(selected.url.protocol, 'https:');
  assert.equal(selected.url.port, '8443');
});

test('rejects non-HTTP gateway protocols before creating a request', () => {
  assert.throws(
    () => selectGatewayHttpClient('ftp://gateway.local/vectors'),
    /Unsupported gateway URL protocol: ftp:/
  );
});
