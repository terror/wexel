(component
  (core module $module
    (func (export "run")
      unreachable
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "run")
    (canon lift (core func $instance "run"))
  )
)
