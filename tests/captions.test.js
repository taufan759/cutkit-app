"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const C = require("../ui/core/captions.js");

test("parseTime menerima format SRT dan sederhana", () => {
  assert.equal(C.parseTime("00:01:02,345"), 62.345);
  assert.equal(C.parseTime("01:02.5"), 62.5);
  assert.equal(C.parseTime("1:00:00.000"), 3600);
  assert.equal(C.parseTime("7"), 7);
  assert.ok(Number.isNaN(C.parseTime("abc")));
});

test("parseSrt: BOM, CRLF, tag, nomor hilang, blok rusak", () => {
  const srt = "﻿1\r\n00:00:00,000 --> 00:00:01,500\r\n<i>Halo</i> semua\r\n\r\n" +
    "2\r\n00:00:01,500 --> 00:00:03,000 X1:10\r\nini baris\r\nkedua\r\n\r\n" +
    "00:00:04,000 --> 00:00:05,000\r\n{\\an8}tanpa nomor\r\n\r\n" +
    "4\r\nbukan waktu\r\nteks\r\n\r\n" +
    "5\r\n00:00:09,000 --> 00:00:08,000\r\nmundur\r\n";
  const cues = C.parseSrt(srt);
  assert.deepEqual(cues, [
    { start: 0, end: 1.5, text: "Halo semua" },
    { start: 1.5, end: 3, text: "ini baris\nkedua" },
    { start: 4, end: 5, text: "tanpa nomor" },
  ]);
});

test("srtToWords membagi waktu cue ke tiap kata secara proporsional", () => {
  const words = C.srtToWords([{ start: 0, end: 3, text: "a bb ccc" }]);
  assert.deepEqual(words.map((w) => w.text), ["a", "bb", "ccc"]);
  assert.equal(words[0].start, 0);
  assert.equal(words[2].end, 3);
  assert.ok(words[0].end - words[0].start < words[2].end - words[2].start);
  for (let i = 1; i < words.length; i++) assert.equal(words[i].start, words[i - 1].end);
});

test("parseWordsJson: format CutKit, larik, beat, alignment ElevenLabs", () => {
  assert.deepEqual(C.parseWordsJson('{"words":[{"text":"halo","start":0,"end":0.4},{"text":"dunia","start":"0.4","end":"0.9"}]}'), [
    { text: "halo", start: 0, end: 0.4 },
    { text: "dunia", start: 0.4, end: 0.9 },
  ]);
  assert.equal(C.parseWordsJson('[{"word":"yo","start":1,"end":2}]')[0].text, "yo");
  const beats = C.parseWordsJson('{"beats":[{"text":"satu dua","start":0,"end":1}]}');
  assert.deepEqual(beats.map((w) => w.text), ["satu", "dua"]);
  assert.equal(beats[1].end, 1);
  const el = C.parseWordsJson(JSON.stringify({ alignment: {
    characters: ["H", "a", "i", " ", "k", "a", "m", "u"],
    character_start_times_seconds: [0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7],
    character_end_times_seconds: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
  } }));
  assert.deepEqual(el, [{ text: "Hai", start: 0, end: 0.3 }, { text: "kamu", start: 0.4, end: 0.8 }]);
  assert.throws(() => C.parseWordsJson("{rusak"), /tidak valid/);
  assert.throws(() => C.parseWordsJson('{"foo":1}'), /words/);
  assert.throws(() => C.parseWordsJson('{"words":[{"text":"x","start":2,"end":1}]}'), /Tidak ada kata/);
});

test("teks manual dibagi ke rentang waktu", () => {
  const w = C.textToWords("  Sebab   sejarah\nitu penting ", 10, 12);
  assert.equal(w.length, 4);
  assert.equal(w[0].start, 10);
  assert.equal(w[3].end, 12);
});

test("frameAt memberi halaman dan kata aktif", () => {
  const words = [{ text: "satu", start: 0, end: 0.5 }, { text: "dua", start: 0.5, end: 1 }, { text: "tiga", start: 1, end: 1.5 }];
  const style = { words_per_page: 2, max_line_chars: 20, uppercase: true };
  assert.deepEqual(C.frameAt(words, style, 0.7).lines, [[{ text: "SATU", active: false }, { text: "DUA", active: true }]]);
  assert.deepEqual(C.frameAt(words, style, 1.2).lines[0][0], { text: "TIGA", active: true });
  assert.equal(C.frameAt(words, style, 3), null);
});

test("ekspor SRT dan JSON kata bisa diimpor ulang", () => {
  const words = [{ text: "satu", start: 0, end: 0.5 }, { text: "dua", start: 0.5, end: 1.25 }, { text: "tiga", start: 3, end: 3.5 }];
  const srt = C.toSrt(words, { words_per_page: 2, max_line_chars: 20 });
  assert.equal(srt, "1\n00:00:00,000 --> 00:00:01,250\nsatu dua\n\n2\n00:00:03,000 --> 00:00:03,500\ntiga\n");
  assert.equal(C.parseSrt(srt).length, 2);
  assert.deepEqual(C.parseWordsJson(C.toWordsJson(words)), words);
});

test("animasi pop: kata aktif mulai 1,25x lalu kembali ke ukuran sorotan dalam 120 ms", () => {
  const st = { highlight_scale: 1.15, animation: "pop" };
  assert.equal(C.activeScale(st, 0), 1.44);
  assert.ok(Math.abs(C.activeScale(st, 0.06) - 1.295) < 1e-9);
  assert.equal(C.activeScale(st, 0.5), 1.15);
  assert.equal(C.activeScale({ highlight_scale: 1.15, animation: "none" }, 0), 1.15);
  const words = [{ text: "a", start: 0, end: 0.5 }, { text: "b", start: 0.5, end: 1 }];
  assert.ok(Math.abs(C.frameAt(words, { words_per_page: 2, max_line_chars: 9 }, 0.55).since - 0.05) < 1e-9);
});
