package org.tatami.engine

import okhttp3.Interceptor
import okhttp3.Response
import org.koitharu.kotatsu.parsers.network.CloudFlareHelper

// Transposé de kotatsu-dl CloudFlareInterceptor, adapté à OkHttp 5
// (close() public au lieu de okhttp3.internal.closeQuietly).
internal class CloudFlareInterceptor : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val response = chain.proceed(request)
        if (CloudFlareHelper.checkResponseForProtection(response) != CloudFlareHelper.PROTECTION_NOT_DETECTED) {
            val url = response.request.url.toString()
            response.close()
            throw CloudFlareProtectionException(url = url)
        }
        return response
    }
}
