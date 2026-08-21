Event Stream
============

This example shows how to parse a WDL document into an event stream, printing that stream to the
terminal.

.. literalinclude:: ../../sprocket_bio/examples/event_stream.py
   :caption: sprocket_bio/examples/event_stream.py

If you have the Python bindings installed, you can run the example yourself:

.. code-block:: console

   $ python -m sprocket_bio.examples.event_stream example.wdl
   SyntaxKind.ROOT_NODE
     SyntaxKind.VERSION_STATEMENT_NODE
       SyntaxKind.VERSION_KEYWORD@0..7 'version'
       SyntaxKind.WHITESPACE@7..8 ' '
       SyntaxKind.VERSION@8..11 '1.3'
     SyntaxKind.WHITESPACE@11..13 '\n\n'
     SyntaxKind.TASK_DEFINITION_NODE
       SyntaxKind.TASK_KEYWORD@13..17 'task'
       SyntaxKind.WHITESPACE@17..18 ' '
       SyntaxKind.IDENT@18..27 'say_hello'
       SyntaxKind.WHITESPACE@27..28 ' '
       SyntaxKind.OPEN_BRACE@28..29 '{'
       SyntaxKind.WHITESPACE@29..34 '\n    '
       SyntaxKind.INPUT_SECTION_NODE
         SyntaxKind.INPUT_KEYWORD@34..39 'input'
         SyntaxKind.WHITESPACE@39..40 ' '
         SyntaxKind.OPEN_BRACE@40..41 '{'
         SyntaxKind.WHITESPACE@41..50 '\n        '
         SyntaxKind.UNBOUND_DECL_NODE
           SyntaxKind.PRIMITIVE_TYPE_NODE
             SyntaxKind.STRING_TYPE_KEYWORD@50..56 'String'
           SyntaxKind.WHITESPACE@56..57 ' '
           SyntaxKind.IDENT@57..65 'greeting'
         SyntaxKind.WHITESPACE@65..70 '\n    '
         SyntaxKind.CLOSE_BRACE@70..71 '}'
       SyntaxKind.WHITESPACE@71..77 '\n\n    '
       SyntaxKind.COMMAND_SECTION_NODE
         SyntaxKind.COMMAND_KEYWORD@77..84 'command'
         SyntaxKind.WHITESPACE@84..85 ' '
         SyntaxKind.OPEN_HEREDOC@85..88 '<<<'
         SyntaxKind.LITERAL_COMMAND_TEXT@88..103 '\n        echo "'
         SyntaxKind.PLACEHOLDER_NODE
           SyntaxKind.PLACEHOLDER_OPEN@103..105 '~{'
           SyntaxKind.ABANDONED
             SyntaxKind.NAME_REF_EXPR_NODE
               SyntaxKind.IDENT@105..113 'greeting'
             SyntaxKind.CLOSE_BRACE@113..114 '}'
           SyntaxKind.LITERAL_COMMAND_TEXT@114..128 ', world!"\n    '
           SyntaxKind.CLOSE_HEREDOC@128..131 '>>>'
         SyntaxKind.WHITESPACE@131..137 '\n\n    '
         SyntaxKind.OUTPUT_SECTION_NODE
           SyntaxKind.OUTPUT_KEYWORD@137..143 'output'
           SyntaxKind.WHITESPACE@143..144 ' '
           SyntaxKind.OPEN_BRACE@144..145 '{'
           SyntaxKind.WHITESPACE@145..154 '\n        '
           SyntaxKind.BOUND_DECL_NODE
             SyntaxKind.PRIMITIVE_TYPE_NODE
               SyntaxKind.STRING_TYPE_KEYWORD@154..160 'String'
             SyntaxKind.WHITESPACE@160..161 ' '
             SyntaxKind.IDENT@161..164 'out'
             SyntaxKind.WHITESPACE@164..165 ' '
             SyntaxKind.ASSIGNMENT@165..166 '='
             SyntaxKind.WHITESPACE@166..167 ' '
             SyntaxKind.ABANDONED
               SyntaxKind.CALL_EXPR_NODE
                 SyntaxKind.IDENT@167..178 'read_string'
                 SyntaxKind.OPEN_PAREN@178..179 '('
                 SyntaxKind.ABANDONED
                   SyntaxKind.CALL_EXPR_NODE
                     SyntaxKind.IDENT@179..185 'stdout'
                     SyntaxKind.OPEN_PAREN@185..186 '('
                     SyntaxKind.CLOSE_PAREN@186..187 ')'
                   SyntaxKind.CLOSE_PAREN@187..188 ')'
               SyntaxKind.WHITESPACE@188..193 '\n    '
               SyntaxKind.CLOSE_BRACE@193..194 '}'
             SyntaxKind.WHITESPACE@194..200 '\n\n    '
             SyntaxKind.REQUIREMENTS_SECTION_NODE
               SyntaxKind.REQUIREMENTS_KEYWORD@200..212 'requirements'
               SyntaxKind.WHITESPACE@212..213 ' '
               SyntaxKind.OPEN_BRACE@213..214 '{'
               SyntaxKind.WHITESPACE@214..223 '\n        '
               SyntaxKind.REQUIREMENTS_ITEM_NODE
                 SyntaxKind.IDENT@223..232 'container'
                 SyntaxKind.COLON@232..233 ':'
                 SyntaxKind.WHITESPACE@233..234 ' '
                 SyntaxKind.ABANDONED
                   SyntaxKind.LITERAL_STRING_NODE
                     SyntaxKind.DOUBLE_QUOTE@234..235 '"'
                     SyntaxKind.LITERAL_STRING_TEXT@235..248 'ubuntu:latest'
                     SyntaxKind.DOUBLE_QUOTE@248..249 '"'
                 SyntaxKind.WHITESPACE@249..254 '\n    '
                 SyntaxKind.CLOSE_BRACE@254..255 '}'
               SyntaxKind.WHITESPACE@255..256 '\n'
               SyntaxKind.CLOSE_BRACE@256..257 '}'
             SyntaxKind.WHITESPACE@257..258 '\n'

.. admonition:: Click to view ``example.wdl`` source
   :collapsible: closed

   .. literalinclude:: ../../sprocket_bio/examples/example.wdl
      :caption: sprocket_bio/examples/example.wdl