import { describe, expect, it } from 'vitest'
import {
  compareVersions,
  findGlibcViolations,
  parseVersion,
  requiredGlibcVersions
} from './glibc-versions.mjs'

// Trimmed `objdump -p` output of a library built on a glibc 2.43 host — the kind of
// file that made the AppImage fail to start on Ubuntu 22.04 (issue #3).
const OBJDUMP_OUTPUT = `
libwebkit2gtk-4.1.so.0:     file format elf64-x86-64

Version References:
  required from libm.so.6:
    0x06969194 0x00 12 GLIBC_2.43
    0x09691a75 0x00 05 GLIBC_2.2.5
  required from libc.so.6:
    0x06969192 0x00 14 GLIBC_2.42
    0x06969194 0x00 09 GLIBC_2.14
    0x09691a75 0x00 03 GLIBC_2.2.5
  required from libstdc++.so.6:
    0x08922974 0x00 07 GLIBCXX_3.4
`

describe('parseVersion', () => {
  it('splits a dotted version into numbers', () => {
    expect(parseVersion('2.2.5')).toEqual([2, 2, 5])
  })

  it('rejects anything that is not a dotted version', () => {
    expect(() => parseVersion('2.x')).toThrow('Not a dotted version')
  })
})

describe('compareVersions', () => {
  it('compares numerically, not as text', () => {
    expect(compareVersions('2.10', '2.9')).toBeGreaterThan(0)
  })

  it('treats missing parts as zero', () => {
    expect(compareVersions('2.35', '2.35.0')).toBe(0)
    expect(compareVersions('2.2.5', '2.2')).toBeGreaterThan(0)
  })
})

describe('requiredGlibcVersions', () => {
  it('lists each glibc version once, oldest first, ignoring GLIBCXX', () => {
    expect(requiredGlibcVersions(OBJDUMP_OUTPUT)).toEqual(['2.2.5', '2.14', '2.42', '2.43'])
  })

  it('returns nothing for a binary with no glibc references', () => {
    expect(requiredGlibcVersions('Version References:\n')).toEqual([])
  })
})

describe('findGlibcViolations', () => {
  const binaries = [
    { file: 'usr/lib/libwebkit2gtk-4.1.so.0', versions: requiredGlibcVersions(OBJDUMP_OUTPUT) },
    { file: 'usr/bin/decktector', versions: ['2.2.5', '2.34'] },
    { file: 'usr/lib/libstatic.so', versions: [] }
  ]

  it('flags binaries that need a newer glibc than allowed', () => {
    expect(findGlibcViolations(binaries, '2.35')).toEqual([
      { file: 'usr/lib/libwebkit2gtk-4.1.so.0', newest: '2.43' }
    ])
  })

  it('passes when every binary fits the limit', () => {
    expect(findGlibcViolations(binaries, '2.43')).toEqual([])
  })
})
