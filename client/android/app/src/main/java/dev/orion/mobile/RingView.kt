package dev.orion.mobile

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.util.AttributeSet
import android.view.View
import kotlin.math.min

/** The status ring: thin track, state arc, inner power ring — drawn, not composed. */
class RingView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    private var secured = false
    private var tracking = false

    private val track = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        color = 0xFF242836.toInt()
    }
    private val arc = Paint(track).apply { color = 0xFF4C7DF0.toInt() }
    private val inner = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        color = 0xFF8E94A8.toInt()
    }

    fun setSecured(on: Boolean) {
        secured = on
        invalidate()
    }
    fun setTracking(on: Boolean) {
        tracking = on
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val cx = width / 2f
        val cy = height / 2f
        val r = min(width, height) / 2f - 14f
        track.strokeWidth = 6f
        canvas.drawCircle(cx, cy, r, track)
        arc.strokeWidth = if (secured || tracking) 8f else 6f
        arc.color = when {
            secured -> 0xFF4C7DF0.toInt()
            tracking -> 0xFF8FA3C8.toInt()
            else -> 0xFF3A415A.toInt()
        }
        canvas.drawArc(cx - r, cy - r, cx + r, cy + r, -90f, 300f, false, arc)
        inner.strokeWidth = 3f
        inner.color = if (secured) 0xFF4C7DF0.toInt() else 0xFF3A415A.toInt()
        canvas.drawCircle(cx, cy, r * 0.62f, inner)
    }
}
