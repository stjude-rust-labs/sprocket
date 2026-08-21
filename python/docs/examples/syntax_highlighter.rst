Syntax Highlighter
==================

This example shows how to use the event stream to make a WDL syntax highlighter that emits HTML.

.. literalinclude:: ../../sprocket_bio/examples/syntax_highlighter.py
   :caption: sprocket_bio/examples/syntax_highlighter.py

If you have the Python bindings installed, you can run the example yourself:

.. code-block:: console

   $ python -m sprocket_bio.examples.syntax_highlighter example.wdl

The emitted HTML renders as the following:

.. raw:: html

   <div style="background: #070A19; color: white; padding: 12px; margin-bottom: 24px"><pre><span style="color: #99A7F1">version</span> <span style="color: #E59CFF">1.3</span>
   
   <span style="color: #99A7F1">task</span> say_hello <span style="color: #BBBBBB">{</span>
       <span style="color: #99A7F1">input</span> <span style="color: #BBBBBB">{</span>
           <span style="color: #BA9CFF">String</span> greeting
       <span style="color: #BBBBBB">}</span>
   
       <span style="color: #99A7F1">command</span> <span style="color: #BBBBBB">&lt;&lt;&lt;</span><span style="color: #E59CFF">
           echo &quot;</span><span style="color: #BBBBBB">~{</span>greeting<span style="color: #BBBBBB">}</span><span style="color: #E59CFF">, world!&quot;
       </span><span style="color: #BBBBBB">&gt;&gt;&gt;</span>
   
       <span style="color: #99A7F1">output</span> <span style="color: #BBBBBB">{</span>
           <span style="color: #BA9CFF">String</span> out <span style="color: #9CB2FF">=</span> read_string<span style="color: #BBBBBB">(</span>stdout<span style="color: #BBBBBB">(</span><span style="color: #BBBBBB">)</span><span style="color: #BBBBBB">)</span>
       <span style="color: #BBBBBB">}</span>
   
       <span style="color: #99A7F1">requirements</span> <span style="color: #BBBBBB">{</span>
           container<span style="color: #BBBBBB">:</span> <span style="color: #E59CFF">&quot;</span><span style="color: #E59CFF">ubuntu:latest</span><span style="color: #E59CFF">&quot;</span>
       <span style="color: #BBBBBB">}</span>
   <span style="color: #BBBBBB">}</span>
   </pre></div>
