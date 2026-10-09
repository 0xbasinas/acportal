package dev.acportal.presentation

import android.content.Context
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning

typealias PairingScan = (onResult:(String)->Unit,onFailure:()->Unit,onCancel:()->Unit)->Unit

internal fun scanPairingQr(context:Context,onResult:(String)->Unit,onFailure:()->Unit,onCancel:()->Unit) {
    try {
        val options=GmsBarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).enableAutoZoom().build()
        GmsBarcodeScanning.getClient(context,options).startScan()
            .addOnSuccessListener {barcode->barcode.rawValue?.let(onResult) ?: onFailure()}
            .addOnFailureListener {onFailure()}
            .addOnCanceledListener {onCancel()}
    } catch(_:Exception) {onFailure()}
}
