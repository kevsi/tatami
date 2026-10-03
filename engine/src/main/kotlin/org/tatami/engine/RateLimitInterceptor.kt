package org.tatami.engine

import okhttp3.Interceptor
import okhttp3.Response

// Un 429 non géré par le parser lui-même devient une erreur typée RATE_LIMITED
// (contrat v0) au lieu d'une liste vide silencieuse.
internal class RateLimitInterceptor : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val response = chain.proceed(chain.request())
        if (response.code == 429) {
            val retryAfter = response.header("Retry-After")?.toLongOrNull()
            response.close()
            throw RateLimitedException(retryAfter)
        }
        return response
    }
}
