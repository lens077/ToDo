// node --test 直接跑 TS（Node 26 内置类型剥离），只测纯文本切分这一层。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { splitLinks, trimUrlTrailing } from './links.ts';

test('普通文本不产生链接', () => {
  assert.deepEqual(splitLinks('今天没有链接。'), [{ text: '今天没有链接。' }]);
  assert.deepEqual(splitLinks(''), []);
});

test('裸 https 链接被切成三段', () => {
  const chunks = splitLinks('见 https://example.com/a 结尾');
  assert.equal(chunks.length, 3);
  assert.equal(chunks[1].href, 'https://example.com/a');
  assert.equal(chunks[1].text, 'https://example.com/a');
});

test('中文句读不算进链接', () => {
  const chunks = splitLinks('原文 https://x.com/foo/status/1。后面还有字');
  assert.equal(chunks[1].href, 'https://x.com/foo/status/1');
  assert.ok(chunks[2].text.startsWith('。'));
});

test('英文句点与逗号也被去掉', () => {
  assert.equal(trimUrlTrailing('https://example.com/a.'), 'https://example.com/a');
  assert.equal(trimUrlTrailing('https://example.com/a),'), 'https://example.com/a');
});

test('URL 自带括号时保留结尾右括号', () => {
  assert.equal(trimUrlTrailing('https://en.wikipedia.org/wiki/Foo_(bar)'), 'https://en.wikipedia.org/wiki/Foo_(bar)');
});

test('一段文本里多个链接', () => {
  const chunks = splitLinks('a https://one.com b https://two.com c');
  assert.equal(chunks.length, 5);
  assert.equal(chunks[1].href, 'https://one.com');
  assert.equal(chunks[3].href, 'https://two.com');
});

test('非 http 前缀不识别', () => {
  assert.deepEqual(splitLinks('ftp://x.com/a'), [{ text: 'ftp://x.com/a' }]);
});
