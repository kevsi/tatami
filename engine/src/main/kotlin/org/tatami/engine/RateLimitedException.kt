package org.tatami.engine

// Transposée de kotatsu-dl RateLimitInterceptor (principe) : un 429 qui remonte
// jusqu'ici devient une erreur typée au lieu d'une liste vide silencieuse.
class RateLimitedException(
    val retryAfterSeconds: Long?,
) : IllegalStateException("Rate limited" + (retryAfterSeconds?.let { " (retry after ${it}s)" } ?: ""))
