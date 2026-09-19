.. _lua_local_module_api:

Local Modules
=============

The `LocalModule` object (which binds the :c:struct:`WpImplModule` C API) provides a way
to load PipeWire modules in the WirePlumber process. Instantiating the object
loads the module, and when the last reference to the returned module object is
dropped, the module is unloaded. Alternatively, the module can be unloaded at a
deterministic point in time with :func:`LocalModule.unload`.

The module is loaded in the *client context*: a secondary ``pw_context`` that
runs on its own thread, so that the module is never held up by whatever
WirePlumber's main loop is doing. See ``support.client-context`` in
:ref:`config_features`. If that feature is disabled, the module is loaded in
WirePlumber's main ``pw_context`` instead.

Constructors
~~~~~~~~~~~~

.. function:: LocalModule(name, arguments, properties)

   Loads the named module with the provided arguments and properties (either of
   which can be ``nil``).

   :param string name: the module name, such as ``"libpipewire-module-loopback"``
   :param string arguments: should be either ``nil`` or a string with the desired
        module arguments
   :param table properties: can be ``nil`` or a table that can be
        :ref:`converted <lua_gobject_lua_to_c>` to :c:struct:`WpProperties`
   :returns: a new LocalModule
   :rtype: LocalModule (:c:struct:`WpImplModule`)
   :since: 0.4.2

Methods
~~~~~~~

.. function:: LocalModule.unload(self)

   Unloads the module immediately, instead of waiting for the last reference to
   the object to be dropped and the garbage collector to finalize it.

   This is useful when the module must be torn down before another module that
   provides objects with the same names is loaded. Relying on the garbage
   collector in that case is not sufficient, because collection happens at an
   unspecified point in time and the two modules may end up loaded
   simultaneously.

   Calling this method more than once has no additional effect and the object
   remains valid afterwards.

   :since: 0.5.16

   Example:

   .. code-block:: lua

      local module = LocalModule("libpipewire-module-loopback", args, {})
      -- ... later, when the module is no longer needed:
      module:unload()
      module = nil
