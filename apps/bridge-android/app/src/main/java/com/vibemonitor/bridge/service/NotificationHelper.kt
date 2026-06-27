package com.vibemonitor.bridge.service

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import com.vibemonitor.bridge.MainActivity
import com.vibemonitor.bridge.R
import com.vibemonitor.bridge.model.PendingAction

object NotificationHelper {
    const val CHANNEL_SERVICE = "vibe_bridge_service"
    const val CHANNEL_ACTION = "vibe_bridge_action"
    const val NOTIFICATION_SERVICE_ID = 1001

    fun ensureChannels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(
                CHANNEL_SERVICE,
                context.getString(R.string.service_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = context.getString(R.string.service_channel_desc)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ACTION,
                context.getString(R.string.action_channel_name),
                NotificationManager.IMPORTANCE_HIGH,
            ).apply {
                description = context.getString(R.string.action_channel_desc)
            },
        )
    }

    fun serviceNotification(context: Context, text: String) =
        NotificationCompat.Builder(context, CHANNEL_SERVICE)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(context.getString(R.string.service_notification_title))
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(activityPendingIntent(context))
            .build()

    fun actionNotification(context: Context, action: PendingAction): android.app.Notification {
        val approve = actionIntent(context, action.id, "approve", 1)
        val deny = actionIntent(context, action.id, "deny", 2)
        return NotificationCompat.Builder(context, CHANNEL_ACTION)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle(action.title)
            .setContentText(action.body.ifBlank { "Agent 等待确认" })
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .addAction(0, "允许", approve)
            .addAction(0, "拒绝", deny)
            .setAutoCancel(true)
            .build()
    }

    private fun activityPendingIntent(context: Context): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
        return PendingIntent.getActivity(
            context,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun actionIntent(
        context: Context,
        actionId: String,
        choice: String,
        requestCode: Int,
    ): PendingIntent {
        val intent = Intent(context, ActionNotificationReceiver::class.java).apply {
            putExtra(EXTRA_ACTION_ID, actionId)
            putExtra(EXTRA_CHOICE, choice)
        }
        return PendingIntent.getBroadcast(
            context,
            requestCode,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    const val EXTRA_ACTION_ID = "action_id"
    const val EXTRA_CHOICE = "choice"
}

class ActionNotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val actionId = intent.getStringExtra(NotificationHelper.EXTRA_ACTION_ID) ?: return
        val choice = intent.getStringExtra(NotificationHelper.EXTRA_CHOICE) ?: return
        BridgeService.respondFromNotification(context, actionId, choice)
    }
}
