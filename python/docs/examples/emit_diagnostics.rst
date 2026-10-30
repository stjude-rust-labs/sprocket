Emit Diagnostics
================

This example shows how to compose diagnostics and emit them to the terminal.

.. literalinclude:: ../../sprocket_bio/examples/emit_diagnostics.py
   :caption: sprocket_bio/examples/emit_diagnostics.py

If you have the Python bindings installed, you can run the example yourself:

.. code-block:: console

   $ python -m sprocket_bio.examples.emit_diagnostics
   error: this is an error
      ┌─ sprocket_bio/examples/example.wdl:8:5
      │  
    8 │ ╭     command <<<
    9 │ │         echo "~{greeting}, world!"
   10 │ │     >>>
      │ ╰───────^
   
   warning: this is a warning
     ┌─ sprocket_bio/examples/example.wdl:1:1
     │
   1 │ version 1.3
     │ ^^^^^^^^^^^ additional details on the warning
     │
     = help: this is the help message
