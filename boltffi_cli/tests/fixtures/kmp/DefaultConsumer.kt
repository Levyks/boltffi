package com.boltffi.defaults

fun main() {
    check(addDefault(right = 10) == 15)
    check(addDefault(right = 10, negate = true) == -15)
    check(addDefault(left = 0, right = 10) == 10)
    check(addDefault(left = 3, right = 10, negate = true) == -13)
}
