package org.actualexpenses.core

import java.math.BigDecimal

/** Shared by form validation and durable saves. Actual deliberately limits amounts to 2^51−1. */
fun parseExpenseAmount(value: String, decimalPlaces: Int): Long {
    require(value.matches(Regex("[0-9]+([.,][0-9]+)?"))) { "Enter a positive expense amount." }
    val minor = try {
        BigDecimal(value.replace(',', '.')).movePointRight(decimalPlaces).longValueExact()
    } catch (_: ArithmeticException) {
        throw IllegalArgumentException("Enter an amount with at most $decimalPlaces decimal places.")
    }
    require(minor in 1..2_251_799_813_685_247L) { "Enter a positive expense amount within Actual's supported range." }
    return minor
}
