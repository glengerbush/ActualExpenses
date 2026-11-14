package org.actualexpenses.core.protocol


const val ZERO_TIMESTAMP = "1970-01-01T00:00:00.000Z-0000-0000000000000000"

data class RemoteBudget(val fileId: String, val groupId: String, val name: String, val encryptKeyId: String? = null)

class ActualException(val kind: Kind, message: String, cause: Throwable? = null) : Exception(message, cause) {
    enum class Kind { AUTHENTICATION, UNSUPPORTED, NETWORK, SERVER, INVALID_DATA, CLOCK }
}

data class HttpResponse(val status: Int, val contentType: String?, val body: ByteArray)
fun interface HttpTransport {
    fun request(url: String, method: String, headers: Map<String, String>, body: ByteArray?): HttpResponse
}
