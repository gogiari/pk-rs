'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const { installEnvironment } = require('../updater.cjs');

test('update bypasses the stopped PK proxy and preserves unrelated proxy settings', () => {
  assert.deepEqual(installEnvironment({
    HTTP_PROXY: 'http://127.0.0.1:3128',
    ALL_PROXY: 'socks5://localhost:1080',
    npm_config_https_proxy: 'http://[::1]:3128',
    HTTPS_PROXY: 'http://company.proxy:8080',
    http_proxy: 'http://localhost:9999',
    NO_PROXY: 'localhost',
    PATH: '/usr/bin',
  }, [3128, 1080]), {
    HTTPS_PROXY: 'http://company.proxy:8080',
    http_proxy: 'http://localhost:9999',
    NO_PROXY: 'localhost',
    PATH: '/usr/bin',
  });
});
