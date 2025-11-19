def to_type_array($spec):
  if $spec == null then []
  elif ($spec | type) == "array" then $spec
  elif ($spec | type) == "string" then [$spec]
  else []
  end;

def match_type($value; $expected):
  if $expected == "string" then ($value | type) == "string"
  elif $expected == "integer" then (($value | type) == "number") and ($value == ($value | floor))
  elif $expected == "number" then ($value | type) == "number"
  elif $expected == "boolean" then ($value | type) == "boolean"
  elif $expected == "object" then ($value | type) == "object"
  elif $expected == "array" then ($value | type) == "array"
  elif $expected == "null" then ($value | type) == "null"
  else false
  end;

def type_matches($value; $spec):
  if $spec == null then true
  else
    (to_type_array($spec)) as $types |
    any($types[]; match_type($value; .))
  end;

def describe($value):
  ($value | type) as $t |
  if $t == "number" then
    if ($value == ($value | floor)) then "integer" else "number" end
  else
    $t
  end;

def canonical:
  if type == "object" then
    (to_entries | sort_by(.key) | map({key: .key, value: (.value | canonical)}) | reduce .[] as $entry ({}; . + { ($entry.key): $entry.value }))
  elif type == "array" then
    map(canonical)
  else
    .
  end;

def unique_items_errors($array; $path):
  reduce range(0; ($array | length)) as $i (
    {seen: [], errors: []};
    ($array[$i] | canonical | tojson) as $marker |
    if (.seen | index($marker)) != null then
      {seen: .seen, errors: (.errors + ["\($path)[\($i)]: duplicate array entry violates uniqueItems"])}
    else
      {seen: (.seen + [$marker]), errors: .errors}
    end
  ) | .errors;

def resolve_ref($root; $ref):
  if ($ref | startswith("#/") | not) then
    error("json_schema_validator: unsupported $ref target " + $ref)
  else
    ($ref | ltrimstr("#/") | split("/")) as $parts |
    reduce $parts[] as $part (
      $root;
      if (type == "object" and has($part)) then .[$part] else error("json_schema_validator: missing $ref segment " + $part) end
    )
  end;

def validate($value; $schema; $root; $path):
  if ($schema | type) != "object" then
    []
  elif $schema["$ref"]? then
    resolve_ref($root; $schema["$ref"]) as $target |
    validate($value; $target; $root; $path)
  else
    ( $schema.type // null ) as $type_spec |
    to_type_array($type_spec) as $allowed_types |
    if ($type_spec != null and (type_matches($value; $type_spec) | not)) then
      ["\($path): expected type \($type_spec | tojson), got \(describe($value))"]
    else
      (
        (if ($schema.const? != null and ($value != $schema.const)) then
         ["\($path): expected const \($schema.const | tojson), got \($value | tojson)"]
       else
         []
       end) +
      (if ($schema.enum? != null and ($schema.enum | index($value)) == null) then
         ["\($path): expected one of \($schema.enum | tojson), got \($value | tojson)"]
       else
         []
       end) +
        (
          ((($schema | has("properties") or has("required") or has("additionalProperties")) or (($allowed_types | index("object")) != null))) as $needs_object |
          if $needs_object then
            if ($value | type) != "object" then
              ["\($path): expected object, got \(describe($value))"]
            else
              ($schema.properties // {}) as $props |
              ($schema.required // []) as $required |
              ($schema.additionalProperties // true) as $additional |
              (
                (
                  reduce $required[]? as $req (
                    [];
                    ($req | tostring) as $req_str |
                    if ($value | has($req_str)) then
                      .
                    else
                      . + ["\($path).\($req_str): missing required property"]
                    end
                  )
                ) +
                (
                  $value
                  | to_entries
                  | reduce .[] as $entry (
                      [];
                      ($entry.key | tostring) as $prop_key |
                      . + (
                        if $props | has($prop_key) then
                          validate($entry.value; $props[$prop_key]; $root; "\($path).\($prop_key)")
                        else
                          if $additional == false then
                            ["\($path).\($prop_key): additional properties not allowed"]
                          elif ($additional | type) == "object" then
                            validate($entry.value; $additional; $root; "\($path).\($prop_key)")
                          else
                            []
                          end
                        end
                      )
                    )
                )
              )
            end
          else
            []
          end
        ) +
        (
          (((($allowed_types | index("array")) != null) or ($schema | has("items")) or ($schema | has("uniqueItems")))) as $needs_array |
          if $needs_array then
            if ($value | type) != "array" then
              ["\($path): expected array, got \(describe($value))"]
            else
              (
                (
                  if (($schema.items // null) | type) == "object" then
                    reduce range(0; ($value | length)) as $i (
                      [];
                      . + validate($value[$i]; $schema.items; $root; "\($path)[\($i)]")
                    )
                  else
                    []
                  end
                ) +
                (
                  if ($schema.uniqueItems // false) == true then
                    unique_items_errors($value; $path)
                  else
                    []
                  end
                )
              )
            end
          else
            []
          end
        )
      )
    end
  end;

($schema[0]) as $schema |
($instance[0]) as $instance |
validate($instance; $schema; $schema; "$")
