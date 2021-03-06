#![cfg(test)]

#[macro_use]
mod macros;

test!(
    basic_variable,
    "$height: 1px;\na {\n  height: $height;\n}\n",
    "a {\n  height: 1px;\n}\n"
);
test!(
    variable_redeclaration,
    "$a: 1px;\n$a: 2px;\na {\n  height: $a;\n}\n",
    "a {\n  height: 2px;\n}\n"
);
test!(
    variable_shadowing,
    "$a: 1px;\n$b: $a;\na {\n  height: $b;\n}\n",
    "a {\n  height: 1px;\n}\n"
);
test!(
    variable_shadowing_val_does_not_change,
    "$a: 1px;\n$b: $a; $a: 2px;\na {\n  height: $b;\n}\n",
    "a {\n  height: 1px;\n}\n"
);
test!(
    variable_shadowing_val_does_not_change_complex,
    "a {\n  color: red;\n}\n$y: before;\n$x: 1 2 $y;\n$y: after;\nfoo {\n  a: $x;\n}",
    "a {\n  color: red;\n}\n\nfoo {\n  a: 1 2 before;\n}\n"
);
test!(
    variable_whitespace,
    "$a   :    1px   ;\na {\n  height: $a;\n}\n",
    "a {\n  height: 1px;\n}\n"
);
test!(
    style_after_variable,
    "$a: 1px;\na {\n  height: $a;\n  color: red;\n}\n",
    "a {\n  height: 1px;\n  color: red;\n}\n"
);
test!(
    literal_and_variable_as_val,
    "$a: 1px;\na {\n  height: 1 $a;\n}\n",
    "a {\n  height: 1 1px;\n}\n"
);
test!(
    literal_and_variable_as_var,
    "$a: 1px;\n$b: 1 $a;\na {\n  height: $b;\n}\n",
    "a {\n  height: 1 1px;\n}\n"
);
test!(
    eats_whitespace_after_variable_value,
    "a {\n  b {\n    $c: red;\n  }\n  color: red;\n}\n",
    "a {\n  color: red;\n}\n"
);
test!(
    variable_changes_through_new_ruleset,
    "a {\n  $c: red;\nb {\n    $c: blue;\n  }\n  color: $c;\n}\n",
    "a {\n  color: blue;\n}\n"
);
test!(
    nested_interpolation,
    "$a: red; a {\n  color: #{#{$a}};\n}\n",
    "a {\n  color: red;\n}\n"
);
test!(
    numbers_in_variable,
    "$var1: red; a {\n  color: $var1;\n}\n",
    "a {\n  color: red;\n}\n"
);
test!(
    default_var_after,
    "$a: red;\n$a: blue !default;\na {\n  color: $a;\n}",
    "a {\n  color: red;\n}\n"
);
test!(
    default_var_before,
    "$a: red !default;\n$a: blue;\na {\n  color: $a;\n}",
    "a {\n  color: blue;\n}\n"
);
test!(
    default_var_whitespace,
    "$a: red     !default          ;\na {\n  color: $a;\n}",
    "a {\n  color: red;\n}\n"
);
test!(
    default_var_inside_rule,
    "a {\n  $a: red;\n  $a: blue !default;\n  color: $a;\n}",
    "a {\n  color: red;\n}\n"
);
test!(
    interpolation_in_variable,
    "$a: #{red};\na {\n  color: $a\n}\n",
    "a {\n  color: red;\n}\n"
);
test!(
    variable_decl_doesnt_end_in_semicolon,
    "a {\n  $a: red\n}\n\nb {\n  color: blue;\n}\n",
    "b {\n  color: blue;\n}\n"
);
test!(
    unicode_in_variables,
    "$vär: foo;\na {\n  color: $vär;\n}\n",
    "a {\n  color: foo;\n}\n"
);
test!(
    variable_does_not_include_interpolation,
    "$input: foo;\na {\n  color: $input#{\"literal\"};\n}\n",
    "a {\n  color: foo literal;\n}\n"
);
test!(
    whitespace_after_variable_name_in_declaration,
    "a {\n  $x : true;\n  color: $x;\n}\n",
    "a {\n  color: true;\n}\n"
);
test!(
    important_in_variable,
    "$a: !important;\n\na {\n  color: $a;\n}\n",
    "a {\n  color: !important;\n}\n"
);
test!(
    important_in_variable_casing,
    "$a: !ImPoRtAnT;\n\na {\n  color: $a;\n}\n",
    "a {\n  color: !important;\n}\n"
);
test!(
    exclamation_in_quoted_string,
    "$a: \"big bang!\";\n\na {\n  color: $a;\n}\n",
    "a {\n  color: \"big bang!\";\n}\n"
);
test!(
    flag_uses_escape_sequence,
    "$a: red;\n\na {\n  $a: green !\\67 lobal;\n}\n\na {\n  color: $a;\n}\n",
    "a {\n  color: green;\n}\n"
);
error!(ends_with_bang, "$a: red !;", "Error: Expected identifier.");
error!(unknown_flag, "$a: red !foo;", "Error: Invalid flag name.");
error!(
    undefined_variable,
    "a {color: $a;}", "Error: Undefined variable."
);
