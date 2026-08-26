(component
  (core module $module
    (func (export "run")
      (loop $forever
        br $forever
      )
    )
  )

  (core instance $instance (instantiate $module))

  (func (export "run")
    (canon lift (core func $instance "run"))
  )
)
