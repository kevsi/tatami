package org.tatami.engine

import org.json.JSONArray
import org.json.JSONObject

internal data class BrandVersion(
    val brand: String,
    val version: String,
)

internal data class BrowserIdentity(
    val brands: List<BrandVersion>,
    val mobile: Boolean?,
    val platform: String?,
    val fullVersionList: List<BrandVersion>,
    val platformVersion: String?,
    val architecture: String?,
    val bitness: String?,
    val model: String?,
    val language: String?,
    val languages: List<String>,
) {
    companion object {
        fun fromJson(json: JSONObject): BrowserIdentity =
            BrowserIdentity(
                brands = json.brandVersions("brands"),
                mobile = json.optionalBoolean("mobile"),
                platform = json.optionalString("platform"),
                fullVersionList = json.brandVersions("fullVersionList"),
                platformVersion = json.optionalString("platformVersion"),
                architecture = json.optionalString("architecture"),
                bitness = json.optionalString("bitness"),
                model = json.optionalString("model"),
                language = json.optionalString("language"),
                languages = json.stringArray("languages"),
            )
    }
}

private fun JSONObject.brandVersions(name: String): List<BrandVersion> =
    optJSONArray(name)?.mapObjects { item ->
        BrandVersion(
            brand = item.getString("brand"),
            version = item.getString("version"),
        )
    } ?: emptyList()

private fun JSONObject.optionalString(name: String): String? =
    if (isNull(name) || !has(name)) null else getString(name)

private fun JSONObject.optionalBoolean(name: String): Boolean? =
    if (isNull(name) || !has(name)) null else getBoolean(name)

private fun JSONObject.stringArray(name: String): List<String> =
    optJSONArray(name)?.mapStrings() ?: emptyList()

private fun JSONArray.mapObjects(transform: (JSONObject) -> BrandVersion): List<BrandVersion> =
    (0 until length()).map { index -> transform(getJSONObject(index)) }

private fun JSONArray.mapStrings(): List<String> =
    (0 until length()).map { index -> getString(index) }
