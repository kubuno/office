package com.kubuno.docs

import android.app.Application
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.disk.DiskCache
import coil3.disk.directory
import coil3.memory.MemoryCache
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.crossfade
import com.kubuno.android.account.AccountManagerBridge
import com.kubuno.docs.net.DocsCallFactory
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

@HiltAndroidApp
class KubunoDocsApp : Application(), SingletonImageLoader.Factory {

    @Inject lateinit var accountBridge: AccountManagerBridge
    @Inject lateinit var callFactory: DocsCallFactory

    override fun onCreate() {
        super.onCreate()
        // Keep the system accounts in step with ours, exactly like the drive,
        // mail, maps and photos apps: whichever app the user opens keeps the
        // shared list current.
        accountBridge.install()
    }

    /**
     * Document covers and inline images are private instance bytes, never public
     * URLs, so every image request has to ride the account-authenticated client
     * the [DocsCallFactory] picks by URL prefix. Caches stay modest: a document
     * list shows far fewer images than a photo gallery.
     */
    override fun newImageLoader(context: PlatformContext): ImageLoader =
        ImageLoader.Builder(context)
            .components {
                add(OkHttpNetworkFetcherFactory(callFactory = { callFactory }))
            }
            .memoryCache {
                MemoryCache.Builder()
                    .maxSizePercent(context, 0.15)
                    .build()
            }
            .diskCache {
                DiskCache.Builder()
                    .directory(cacheDir.resolve("doc_images"))
                    .maxSizeBytes(64L * 1024 * 1024)
                    .build()
            }
            .crossfade(true)
            .build()
}
