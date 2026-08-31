(component
  (core module $module
    (func $start
      (loop $forever
        br $forever
      )
    )

    (start $start)
  )

  (core instance $instance (instantiate $module))
)
