# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1](https://github.com/nodeve-com/schema-emit/compare/schema-emit-v0.8.0...schema-emit-v0.8.1) - 2026-11-19

### Added

- with_binary_drops_undeclared leaves an open model's undeclared keys out of the positional form

## [0.8.0](https://github.com/nodeve-com/schema-emit/compare/schema-emit-v0.7.2...schema-emit-v0.8.0) - 2026-11-18

### Added

- [**breaking**] Schema::named holds each shared definition the root reaches, beside the schema's own

## [0.7.2](https://github.com/nodeve-com/schema-emit/compare/schema-emit-v0.7.1...schema-emit-v0.7.2) - 2026-11-16

### Other

- a value's type and its Rust text travel as one argument
- decode_value and encode_value
- rust, decode_value and encode_value, a type's text, reader and writer apart from any model

## [0.7.1](https://github.com/nodeve-com/schema-emit/compare/schema-emit-v0.7.0...schema-emit-v0.7.1) - 2026-11-16

### Fixed

- a dynamic reference filled with a type that is no object holds the type itself

### Other

- a type parameter holds the filling schema's type, and its four functions

## [0.7.0](https://github.com/nodeve-com/schema-emit/compare/schema-emit-v0.6.2...schema-emit-v0.7.0) - 2026-11-15

### Other

- json_schema::join and defs
- defs and join, a document's own definitions and a second's joined among them
- a filled schema names each model's struct; named holds the entries a schema reads
- A rooted schema names each $defs entry its root reaches
- Fields filled later; a filled schema at any depth; each $defs entry named
- fields filled later; a filled model's fields at any depth; named types
- Call a model's open part a dynamic reference, its Rust form a type parameter
- Take pattern-emit and any-value from crates.io; release through release-plz
