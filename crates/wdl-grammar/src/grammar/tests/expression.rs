use pest::consumes_to;
use pest::parses_to;

use crate::Parser as WdlParser;
use crate::Rule;

mod core;
mod infix;
mod prefix;
mod suffix;

#[test]
fn it_parses_an_extremely_complicated_expression() {
    parses_to! {
        parser: WdlParser,
        input: "if
    if true == false && 2 != 1 then
        (
            object {a: true}.a ||
            !(true, false)[0]
        )
    else
        -struct {b: 10}.b
then
    [0, 1, 2, 3e10][if true then 2 else 1] ||
    [0, 0432, 0xF2, -3e+10](zulu)
else
    false
",
        rule: Rule::expression,
        tokens: [
            expression(0, 258, [
                core(0, 258, [
                  r#if(0, 258, [
                    WHITESPACE(2, 3, [
                      INDENT(2, 3, [
                        SPACE(2, 3),
                      ]),
                    ]),
                    WHITESPACE(3, 4, [
                      LINE_ENDING(3, 4, [
                        NEWLINE(3, 4),
                      ]),
                    ]),
                    WHITESPACE(4, 5, [
                      INDENT(4, 5, [
                        SPACE(4, 5),
                      ]),
                    ]),
                    WHITESPACE(5, 6, [
                      INDENT(5, 6, [
                        SPACE(5, 6),
                      ]),
                    ]),
                    WHITESPACE(6, 7, [
                      INDENT(6, 7, [
                        SPACE(6, 7),
                      ]),
                    ]),
                    WHITESPACE(7, 8, [
                      INDENT(7, 8, [
                        SPACE(7, 8),
                      ]),
                    ]),
                    expression(8, 158, [
                      core(8, 158, [
                        r#if(8, 158, [
                          WHITESPACE(10, 11, [
                            INDENT(10, 11, [
                              SPACE(10, 11),
                            ]),
                          ]),
                          // `true == false && 2 != 1`
                          expression(11, 34, [
                            // `true`
                            core(11, 15, [
                              // `true`
                              literal(11, 15, [
                                // `true`
                                boolean(11, 15),
                              ]),
                            ]),
                            WHITESPACE(15, 16, [
                              INDENT(15, 16, [
                                SPACE(15, 16),
                              ]),
                            ]),
                            // `==`
                            infix(16, 18, [
                              // `==`
                              eq(16, 18),
                            ]),
                            WHITESPACE(18, 19, [
                              INDENT(18, 19, [
                                SPACE(18, 19),
                              ]),
                            ]),
                            // `false`
                            core(19, 24, [
                              // `false`
                              literal(19, 24, [
                                // `false`
                                boolean(19, 24),
                              ]),
                            ]),
                            WHITESPACE(24, 25, [
                              INDENT(24, 25, [
                                SPACE(24, 25),
                              ]),
                            ]),
                            // `&&`
                            infix(25, 27, [
                              // `&&`
                              and(25, 27),
                            ]),
                            WHITESPACE(27, 28, [
                              INDENT(27, 28, [
                                SPACE(27, 28),
                              ]),
                            ]),
                            // `2`
                            core(28, 29, [
                              // `2`
                              literal(28, 29, [
                                // `2`
                                integer(28, 29, [
                                  // `2`
                                  integer_decimal(28, 29),
                                ]),
                              ]),
                            ]),
                            WHITESPACE(29, 30, [
                              INDENT(29, 30, [
                                SPACE(29, 30),
                              ]),
                            ]),
                            // `!=`
                            infix(30, 32, [
                              // `!=`
                              neq(30, 32),
                            ]),
                            WHITESPACE(32, 33, [
                              INDENT(32, 33, [
                                SPACE(32, 33),
                              ]),
                            ]),
                            // `1`
                            core(33, 34, [
                              // `1`
                              literal(33, 34, [
                                // `1`
                                integer(33, 34, [
                                  // `1`
                                  integer_decimal(33, 34),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(34, 35, [
                            INDENT(34, 35, [
                              SPACE(34, 35),
                            ]),
                          ]),
                          WHITESPACE(39, 40, [
                            LINE_ENDING(39, 40, [
                              NEWLINE(39, 40),
                            ]),
                          ]),
                          WHITESPACE(40, 41, [
                            INDENT(40, 41, [
                              SPACE(40, 41),
                            ]),
                          ]),
                          WHITESPACE(41, 42, [
                            INDENT(41, 42, [
                              SPACE(41, 42),
                            ]),
                          ]),
                          WHITESPACE(42, 43, [
                            INDENT(42, 43, [
                              SPACE(42, 43),
                            ]),
                          ]),
                          WHITESPACE(43, 44, [
                            INDENT(43, 44, [
                              SPACE(43, 44),
                            ]),
                          ]),
                          WHITESPACE(44, 45, [
                            INDENT(44, 45, [
                              SPACE(44, 45),
                            ]),
                          ]),
                          WHITESPACE(45, 46, [
                            INDENT(45, 46, [
                              SPACE(45, 46),
                            ]),
                          ]),
                          WHITESPACE(46, 47, [
                            INDENT(46, 47, [
                              SPACE(46, 47),
                            ]),
                          ]),
                          WHITESPACE(47, 48, [
                            INDENT(47, 48, [
                              SPACE(47, 48),
                            ]),
                          ]),
                          expression(48, 123, [
                            core(48, 123, [
                              group(48, 123, [
                                WHITESPACE(49, 50, [
                                  LINE_ENDING(49, 50, [
                                    NEWLINE(49, 50),
                                  ]),
                                ]),
                                WHITESPACE(50, 51, [
                                  INDENT(50, 51, [
                                    SPACE(50, 51),
                                  ]),
                                ]),
                                WHITESPACE(51, 52, [
                                  INDENT(51, 52, [
                                    SPACE(51, 52),
                                  ]),
                                ]),
                                WHITESPACE(52, 53, [
                                  INDENT(52, 53, [
                                    SPACE(52, 53),
                                  ]),
                                ]),
                                WHITESPACE(53, 54, [
                                  INDENT(53, 54, [
                                    SPACE(53, 54),
                                  ]),
                                ]),
                                WHITESPACE(54, 55, [
                                  INDENT(54, 55, [
                                    SPACE(54, 55),
                                  ]),
                                ]),
                                WHITESPACE(55, 56, [
                                  INDENT(55, 56, [
                                    SPACE(55, 56),
                                  ]),
                                ]),
                                WHITESPACE(56, 57, [
                                  INDENT(56, 57, [
                                    SPACE(56, 57),
                                  ]),
                                ]),
                                WHITESPACE(57, 58, [
                                  INDENT(57, 58, [
                                    SPACE(57, 58),
                                  ]),
                                ]),
                                WHITESPACE(58, 59, [
                                  INDENT(58, 59, [
                                    SPACE(58, 59),
                                  ]),
                                ]),
                                WHITESPACE(59, 60, [
                                  INDENT(59, 60, [
                                    SPACE(59, 60),
                                  ]),
                                ]),
                                WHITESPACE(60, 61, [
                                  INDENT(60, 61, [
                                    SPACE(60, 61),
                                  ]),
                                ]),
                                WHITESPACE(61, 62, [
                                  INDENT(61, 62, [
                                    SPACE(61, 62),
                                  ]),
                                ]),
                                expression(62, 113, [
                                  // `object {a: true}`
                                  core(62, 78, [
                                    // `object {a: true}`
                                    object_literal(62, 78, [
                                      WHITESPACE(68, 69, [
                                        INDENT(68, 69, [
                                          SPACE(68, 69),
                                        ]),
                                      ]),
                                      // `a: true`
                                      identifier_based_kv_pair(70, 77, [
                                        // `a`
                                        identifier_based_kv_key(70, 71, [
                                          // `a`
                                          identifier(70, 71),
                                        ]),
                                        WHITESPACE(72, 73, [
                                          INDENT(72, 73, [
                                            SPACE(72, 73),
                                          ]),
                                        ]),
                                        // `true`
                                        kv_value(73, 77, [
                                          // `true`
                                          expression(73, 77, [
                                            // `true`
                                            core(73, 77, [
                                              // `true`
                                              literal(73, 77, [
                                                // `true`
                                                boolean(73, 77),
                                              ]),
                                            ]),
                                          ]),
                                        ]),
                                      ]),
                                    ]),
                                  ]),
                                  // `.a`
                                  postfix(78, 80, [
                                    // `.a`
                                    member(78, 80, [
                                      // `a`
                                      identifier(79, 80),
                                    ]),
                                  ]),
                                  WHITESPACE(80, 81, [
                                    INDENT(80, 81, [
                                      SPACE(80, 81),
                                    ]),
                                  ]),
                                  // `||`
                                  infix(81, 83, [
                                    // `||`
                                    or(81, 83),
                                  ]),
                                  WHITESPACE(83, 84, [
                                    LINE_ENDING(83, 84, [
                                      NEWLINE(83, 84),
                                    ]),
                                  ]),
                                  WHITESPACE(84, 85, [
                                    INDENT(84, 85, [
                                      SPACE(84, 85),
                                    ]),
                                  ]),
                                  WHITESPACE(85, 86, [
                                    INDENT(85, 86, [
                                      SPACE(85, 86),
                                    ]),
                                  ]),
                                  WHITESPACE(86, 87, [
                                    INDENT(86, 87, [
                                      SPACE(86, 87),
                                    ]),
                                  ]),
                                  WHITESPACE(87, 88, [
                                    INDENT(87, 88, [
                                      SPACE(87, 88),
                                    ]),
                                  ]),
                                  WHITESPACE(88, 89, [
                                    INDENT(88, 89, [
                                      SPACE(88, 89),
                                    ]),
                                  ]),
                                  WHITESPACE(89, 90, [
                                    INDENT(89, 90, [
                                      SPACE(89, 90),
                                    ]),
                                  ]),
                                  WHITESPACE(90, 91, [
                                    INDENT(90, 91, [
                                      SPACE(90, 91),
                                    ]),
                                  ]),
                                  WHITESPACE(91, 92, [
                                    INDENT(91, 92, [
                                      SPACE(91, 92),
                                    ]),
                                  ]),
                                  WHITESPACE(92, 93, [
                                    INDENT(92, 93, [
                                      SPACE(92, 93),
                                    ]),
                                  ]),
                                  WHITESPACE(93, 94, [
                                    INDENT(93, 94, [
                                      SPACE(93, 94),
                                    ]),
                                  ]),
                                  WHITESPACE(94, 95, [
                                    INDENT(94, 95, [
                                      SPACE(94, 95),
                                    ]),
                                  ]),
                                  WHITESPACE(95, 96, [
                                    INDENT(95, 96, [
                                      SPACE(95, 96),
                                    ]),
                                  ]),
                                  // `!`
                                  prefix(96, 97, [
                                    // `!`
                                    negation(96, 97),
                                  ]),
                                  // `(true, false)`
                                  core(97, 110, [
                                    // `(true, false)`
                                    pair_literal(97, 110, [
                                      // `true`
                                      expression(98, 102, [
                                        // `true`
                                        core(98, 102, [
                                          // `true`
                                          literal(98, 102, [
                                            // `true`
                                            boolean(98, 102),
                                          ]),
                                        ]),
                                      ]),
                                      WHITESPACE(103, 104, [
                                        INDENT(103, 104, [
                                          SPACE(103, 104),
                                        ]),
                                      ]),
                                      // `false`
                                      expression(104, 109, [
                                        // `false`
                                        core(104, 109, [
                                          // `false`
                                          literal(104, 109, [
                                            // `false`
                                            boolean(104, 109),
                                          ]),
                                        ]),
                                      ]),
                                    ]),
                                  ]),
                                  // `[0]`
                                  postfix(110, 113, [
                                    // `[0]`
                                    index(110, 113, [
                                      // `0`
                                      expression(111, 112, [
                                        // `0`
                                        core(111, 112, [
                                          // `0`
                                          literal(111, 112, [
                                            // `0`
                                            integer(111, 112, [
                                              // `0`
                                              integer_decimal(111, 112),
                                            ]),
                                          ]),
                                        ]),
                                      ]),
                                    ]),
                                  ]),
                                ]),
                                WHITESPACE(113, 114, [
                                  LINE_ENDING(113, 114, [
                                    NEWLINE(113, 114),
                                  ]),
                                ]),
                                WHITESPACE(114, 115, [
                                  INDENT(114, 115, [
                                    SPACE(114, 115),
                                  ]),
                                ]),
                                WHITESPACE(115, 116, [
                                  INDENT(115, 116, [
                                    SPACE(115, 116),
                                  ]),
                                ]),
                                WHITESPACE(116, 117, [
                                  INDENT(116, 117, [
                                    SPACE(116, 117),
                                  ]),
                                ]),
                                WHITESPACE(117, 118, [
                                  INDENT(117, 118, [
                                    SPACE(117, 118),
                                  ]),
                                ]),
                                WHITESPACE(118, 119, [
                                  INDENT(118, 119, [
                                    SPACE(118, 119),
                                  ]),
                                ]),
                                WHITESPACE(119, 120, [
                                  INDENT(119, 120, [
                                    SPACE(119, 120),
                                  ]),
                                ]),
                                WHITESPACE(120, 121, [
                                  INDENT(120, 121, [
                                    SPACE(120, 121),
                                  ]),
                                ]),
                                WHITESPACE(121, 122, [
                                  INDENT(121, 122, [
                                    SPACE(121, 122),
                                  ]),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(123, 124, [
                            LINE_ENDING(123, 124, [
                              NEWLINE(123, 124),
                            ]),
                          ]),
                          WHITESPACE(124, 125, [
                            INDENT(124, 125, [
                              SPACE(124, 125),
                            ]),
                          ]),
                          WHITESPACE(125, 126, [
                            INDENT(125, 126, [
                              SPACE(125, 126),
                            ]),
                          ]),
                          WHITESPACE(126, 127, [
                            INDENT(126, 127, [
                              SPACE(126, 127),
                            ]),
                          ]),
                          WHITESPACE(127, 128, [
                            INDENT(127, 128, [
                              SPACE(127, 128),
                            ]),
                          ]),
                          WHITESPACE(132, 133, [
                            LINE_ENDING(132, 133, [
                              NEWLINE(132, 133),
                            ]),
                          ]),
                          WHITESPACE(133, 134, [
                            INDENT(133, 134, [
                              SPACE(133, 134),
                            ]),
                          ]),
                          WHITESPACE(134, 135, [
                            INDENT(134, 135, [
                              SPACE(134, 135),
                            ]),
                          ]),
                          WHITESPACE(135, 136, [
                            INDENT(135, 136, [
                              SPACE(135, 136),
                            ]),
                          ]),
                          WHITESPACE(136, 137, [
                            INDENT(136, 137, [
                              SPACE(136, 137),
                            ]),
                          ]),
                          WHITESPACE(137, 138, [
                            INDENT(137, 138, [
                              SPACE(137, 138),
                            ]),
                          ]),
                          WHITESPACE(138, 139, [
                            INDENT(138, 139, [
                              SPACE(138, 139),
                            ]),
                          ]),
                          WHITESPACE(139, 140, [
                            INDENT(139, 140, [
                              SPACE(139, 140),
                            ]),
                          ]),
                          WHITESPACE(140, 141, [
                            INDENT(140, 141, [
                              SPACE(140, 141),
                            ]),
                          ]),
                          // `-struct {b: 10}.b`
                          expression(141, 158, [
                            // `-`
                            prefix(141, 142, [
                              // `-`
                              unary_signed(141, 142),
                            ]),
                            // `struct {b: 10}`
                            core(142, 156, [
                              // `struct {b: 10}`
                              struct_literal(142, 156, [
                                // `struct`
                                identifier(142, 148),
                                WHITESPACE(148, 149, [
                                  INDENT(148, 149, [
                                    SPACE(148, 149),
                                  ]),
                                ]),
                                // `b: 10`
                                identifier_based_kv_pair(150, 155, [
                                  // `b`
                                  identifier_based_kv_key(150, 151, [
                                    // `b`
                                    identifier(150, 151),
                                  ]),
                                  WHITESPACE(152, 153, [
                                    INDENT(152, 153, [
                                      SPACE(152, 153),
                                    ]),
                                  ]),
                                  // `10`
                                  kv_value(153, 155, [
                                    // `10`
                                    expression(153, 155, [
                                      // `10`
                                      core(153, 155, [
                                        // `10`
                                        literal(153, 155, [
                                          // `10`
                                          integer(153, 155, [
                                            // `10`
                                            integer_decimal(153, 155),
                                          ]),
                                        ]),
                                      ]),
                                    ]),
                                  ]),
                                ]),
                              ]),
                            ]),
                            // `.b`
                            postfix(156, 158, [
                              // `.b`
                              member(156, 158, [
                                // `b`
                                identifier(157, 158),
                              ]),
                            ]),
                          ]),
                        ]),
                      ]),
                    ]),
                    WHITESPACE(158, 159, [
                      LINE_ENDING(158, 159, [
                        NEWLINE(158, 159),
                      ]),
                    ]),
                    WHITESPACE(163, 164, [
                      LINE_ENDING(163, 164, [
                        NEWLINE(163, 164),
                      ]),
                    ]),
                    WHITESPACE(164, 165, [
                      INDENT(164, 165, [
                        SPACE(164, 165),
                      ]),
                    ]),
                    WHITESPACE(165, 166, [
                      INDENT(165, 166, [
                        SPACE(165, 166),
                      ]),
                    ]),
                    WHITESPACE(166, 167, [
                      INDENT(166, 167, [
                        SPACE(166, 167),
                      ]),
                    ]),
                    WHITESPACE(167, 168, [
                      INDENT(167, 168, [
                        SPACE(167, 168),
                      ]),
                    ]),
                    expression(168, 243, [
                      // `[0, 1, 2, 3e10]`
                      core(168, 183, [
                        // `[0, 1, 2, 3e10]`
                        array_literal(168, 183, [
                          // `0`
                          expression(169, 170, [
                            // `0`
                            core(169, 170, [
                              // `0`
                              literal(169, 170, [
                                // `0`
                                integer(169, 170, [
                                  // `0`
                                  integer_decimal(169, 170),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(171, 172, [
                            INDENT(171, 172, [
                              SPACE(171, 172),
                            ]),
                          ]),
                          // `1`
                          expression(172, 173, [
                            // `1`
                            core(172, 173, [
                              // `1`
                              literal(172, 173, [
                                // `1`
                                integer(172, 173, [
                                  // `1`
                                  integer_decimal(172, 173),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(174, 175, [
                            INDENT(174, 175, [
                              SPACE(174, 175),
                            ]),
                          ]),
                          // `2`
                          expression(175, 176, [
                            // `2`
                            core(175, 176, [
                              // `2`
                              literal(175, 176, [
                                // `2`
                                integer(175, 176, [
                                  // `2`
                                  integer_decimal(175, 176),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(177, 178, [
                            INDENT(177, 178, [
                              SPACE(177, 178),
                            ]),
                          ]),
                          // `3e10`
                          expression(178, 182, [
                            // `3e10`
                            core(178, 182, [
                              // `3e10`
                              literal(178, 182, [
                                // `3e10`
                                float(178, 182, [
                                  // `3e10`
                                  float_simple(178, 182),
                                ]),
                              ]),
                            ]),
                          ]),
                        ]),
                      ]),
                      // `[if true then 2 else 1]`
                      postfix(183, 206, [
                        // `[if true then 2 else 1]`
                        index(183, 206, [
                          // `if true then 2 else 1`
                          expression(184, 205, [
                            // `if true then 2 else 1`
                            core(184, 205, [
                              // `if true then 2 else 1`
                              r#if(184, 205, [
                                WHITESPACE(186, 187, [
                                  INDENT(186, 187, [
                                    SPACE(186, 187),
                                  ]),
                                ]),
                                // `true`
                                expression(187, 191, [
                                  // `true`
                                  core(187, 191, [
                                    // `true`
                                    literal(187, 191, [
                                      // `true`
                                      boolean(187, 191),
                                    ]),
                                  ]),
                                ]),
                                WHITESPACE(191, 192, [
                                  INDENT(191, 192, [
                                    SPACE(191, 192),
                                  ]),
                                ]),
                                WHITESPACE(196, 197, [
                                  INDENT(196, 197, [
                                    SPACE(196, 197),
                                  ]),
                                ]),
                                // `2`
                                expression(197, 198, [
                                  // `2`
                                  core(197, 198, [
                                    // `2`
                                    literal(197, 198, [
                                      // `2`
                                      integer(197, 198, [
                                        // `2`
                                        integer_decimal(197, 198),
                                      ]),
                                    ]),
                                  ]),
                                ]),
                                WHITESPACE(198, 199, [
                                  INDENT(198, 199, [
                                    SPACE(198, 199),
                                  ]),
                                ]),
                                WHITESPACE(203, 204, [
                                  INDENT(203, 204, [
                                    SPACE(203, 204),
                                  ]),
                                ]),
                                // `1`
                                expression(204, 205, [
                                  // `1`
                                  core(204, 205, [
                                    // `1`
                                    literal(204, 205, [
                                      // `1`
                                      integer(204, 205, [
                                        // `1`
                                        integer_decimal(204, 205),
                                      ]),
                                    ]),
                                  ]),
                                ]),
                              ]),
                            ]),
                          ]),
                        ]),
                      ]),
                      WHITESPACE(206, 207, [
                        INDENT(206, 207, [
                          SPACE(206, 207),
                        ]),
                      ]),
                      // `||`
                      infix(207, 209, [
                        // `||`
                        or(207, 209),
                      ]),
                      WHITESPACE(209, 210, [
                        LINE_ENDING(209, 210, [
                          NEWLINE(209, 210),
                        ]),
                      ]),
                      WHITESPACE(210, 211, [
                        INDENT(210, 211, [
                          SPACE(210, 211),
                        ]),
                      ]),
                      WHITESPACE(211, 212, [
                        INDENT(211, 212, [
                          SPACE(211, 212),
                        ]),
                      ]),
                      WHITESPACE(212, 213, [
                        INDENT(212, 213, [
                          SPACE(212, 213),
                        ]),
                      ]),
                      WHITESPACE(213, 214, [
                        INDENT(213, 214, [
                          SPACE(213, 214),
                        ]),
                      ]),
                      // `[0, 0432, 0xF2, -3e+10]`
                      core(214, 237, [
                        // `[0, 0432, 0xF2, -3e+10]`
                        array_literal(214, 237, [
                          // `0`
                          expression(215, 216, [
                            // `0`
                            core(215, 216, [
                              // `0`
                              literal(215, 216, [
                                // `0`
                                integer(215, 216, [
                                  // `0`
                                  integer_decimal(215, 216),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(217, 218, [
                            INDENT(217, 218, [
                              SPACE(217, 218),
                            ]),
                          ]),
                          // `0432`
                          expression(218, 222, [
                            // `0432`
                            core(218, 222, [
                              // `0432`
                              literal(218, 222, [
                                // `0432`
                                integer(218, 222, [
                                  // `0432`
                                  integer_octal(218, 222),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(223, 224, [
                            INDENT(223, 224, [
                              SPACE(223, 224),
                            ]),
                          ]),
                          // `0xF2`
                          expression(224, 228, [
                            // `0xF2`
                            core(224, 228, [
                              // `0xF2`
                              literal(224, 228, [
                                // `0xF2`
                                integer(224, 228, [
                                  // `0xF2`
                                  integer_hex(224, 228),
                                ]),
                              ]),
                            ]),
                          ]),
                          WHITESPACE(229, 230, [
                            INDENT(229, 230, [
                              SPACE(229, 230),
                            ]),
                          ]),
                          // `-3e+10`
                          expression(230, 236, [
                            // `-`
                            prefix(230, 231, [
                              // `-`
                              unary_signed(230, 231),
                            ]),
                            // `3e+10`
                            core(231, 236, [
                              // `3e+10`
                              literal(231, 236, [
                                // `3e+10`
                                float(231, 236, [
                                  // `3e+10`
                                  float_simple(231, 236),
                                ]),
                              ]),
                            ]),
                          ]),
                        ]),
                      ]),
                      // `(zulu)`
                      postfix(237, 243, [
                        // `(zulu)`
                        apply(237, 243, [
                          // `zulu`
                          expression(238, 242, [
                            // `zulu`
                            core(238, 242, [
                              // `zulu`
                              literal(238, 242, [
                                // `zulu`
                                identifier(238, 242),
                              ]),
                            ]),
                          ]),
                        ]),
                      ]),
                    ]),
                    WHITESPACE(243, 244, [
                      LINE_ENDING(243, 244, [
                        NEWLINE(243, 244),
                      ]),
                    ]),
                    WHITESPACE(248, 249, [
                      LINE_ENDING(248, 249, [
                        NEWLINE(248, 249),
                      ]),
                    ]),
                    WHITESPACE(249, 250, [
                      INDENT(249, 250, [
                        SPACE(249, 250),
                      ]),
                    ]),
                    WHITESPACE(250, 251, [
                      INDENT(250, 251, [
                        SPACE(250, 251),
                      ]),
                    ]),
                    WHITESPACE(251, 252, [
                      INDENT(251, 252, [
                        SPACE(251, 252),
                      ]),
                    ]),
                    WHITESPACE(252, 253, [
                      INDENT(252, 253, [
                        SPACE(252, 253),
                      ]),
                    ]),
                    // `false`
                    expression(253, 258, [
                      // `false`
                      core(253, 258, [
                        // `false`
                        literal(253, 258, [
                          // `false`
                          boolean(253, 258),
                        ]),
                      ]),
                    ]),
                  ]),
                ]),
              ])
        ]
    }
}
