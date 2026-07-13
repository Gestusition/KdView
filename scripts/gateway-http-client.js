'use strict';

const http = require('node:http');
const https = require('node:https');

/**
 * Parse a gateway URL and select the matching Node transport.
 *
 * Gateway integrations intentionally support only HTTP(S). Rejecting every
 * other scheme before a request is created prevents a local `http://` gateway
 * from accidentally being sent through the TLS client while keeping HTTPS
 * (including self-signed appliance certificates) available.
 *
 * @param {string | URL} urlLike
 * @returns {{ url: URL, client: typeof http | typeof https }}
 */
function selectGatewayHttpClient(urlLike) {
  const url = urlLike instanceof URL ? urlLike : new URL(urlLike);

  if (url.protocol === 'http:') {
    return { url, client: http };
  }
  if (url.protocol === 'https:') {
    return { url, client: https };
  }

  throw new TypeError(`Unsupported gateway URL protocol: ${url.protocol}`);
}

module.exports = { selectGatewayHttpClient };
