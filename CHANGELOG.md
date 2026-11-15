# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
