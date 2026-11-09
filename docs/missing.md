# Missing

What schema-emit refuses today that JSON Schema allows, largest gap first. schema-emit refuses each with its reason, never checking it wrong. `cargo test -p schema-emit --test suite -- --nocapture` counts the instances each reason costs. An instance no typed value holds, such as text where the schema says number, is by design, not missing.

## Keys no field names

Of an open object, what remains: `patternProperties` matching a field's name, `uniqueItems` over such objects, and a subschema judging the value under a key no keyword types, as a `dependentSchemas` subschema does. So too a branch (`anyOf`, `oneOf`, `if`) over an object holding a count, `dependentRequired`, or `additionalProperties`.

## Unevaluated

What depends on which subschema passed: `unevaluatedProperties` beside a branch that reads keys nothing else does, and `unevaluatedItems` beside `contains`, `$ref`, or a subschema reading items. So too `unevaluatedProperties` as a schema, or beside `patternProperties`, which reach keys no field names.

## References beyond the document

`$dynamicRef`, and a `$ref` to another file or URL, which nothing fetches. A recursive `$ref` beside a keyword that checks, and a `$schema` other than 2020-12's.

## Values of more than one kind

`type` as a list of types, and a schema with no `type` whose `oneOf` names branches, or whose `enum` and `const` members are of more than one kind. An untyped `anyOf` is a union. So too `allOf` of different types, and `not` of an untyped schema or a boolean schema beside an object, list, or map.

## Lists and objects as choices

`const` and `enum` whose object names a key no field holds, since a value may carry keys its model lacks. A choice of a map, and of a tuple with trailing items.

## Smaller gaps

- **`pattern`**: refused where pattern-emit's ECMA-262 guard refuses or its table passes the cap: lookaround, backreferences, inline flags, `\p` beyond a general category by short name, and Unicode classes wider than a few thousand characters (`\p{L}`).
- **Branches**: `items`, `prefixItems`, or `additionalProperties` in an untyped branch over a list or map.
- **Unknown keywords**: 2020-12 ignores a keyword it doesn't know. schema-emit refuses any it wasn't asked to carry.
- **Defaults**: a `default` of a list, an object, a timestamp, a duration, bytes, or a nullable type, one on a schema with no `type`, one beside a recursive `$ref`, and one beside a check other than a choice, a bound, `multipleOf`, a length, or a pattern, which lowering can't judge it by.
- **Arrays**: an array without `items`, `uniqueItems` of a tuple with trailing items, and `contains` of another type than `items`.
