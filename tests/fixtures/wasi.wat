(component
  (type $environment (instance
    (export "get-arguments" (func (result (list string))))
    (export "get-environment" (func (result (list (tuple string string)))))
    (export "initial-cwd" (func (result (option string))))
  ))

  (import "wasi:cli/environment@0.2.12"
    (instance $environment (type $environment))
  )

  (core module $module
    (func (export "answer") (result i32)
      i32.const 42
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "answer") (result u32)
    (canon lift (core func $instance "answer"))
  )
)
