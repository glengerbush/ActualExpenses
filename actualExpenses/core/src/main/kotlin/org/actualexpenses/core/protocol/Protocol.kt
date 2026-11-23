package org.actualexpenses.core.protocol


const val ZERO_TIMESTAMP = "1970-01-01T00:00:00.000Z-0000-0000000000000000"

data class HttpResponse(val status: Int, val contentType: String?, val body: ByteArray)
fun interface HttpTransport {
    fun request(url: String, method: String, headers: Map<String, String>, body: ByteArray?): HttpResponse
}
