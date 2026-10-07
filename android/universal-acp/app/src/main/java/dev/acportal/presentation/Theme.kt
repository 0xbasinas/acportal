package dev.acportal.presentation

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

val ConnectedGreen=Color(0xFF72D5B0)
private val Dark=darkColorScheme(primary=Color(0xFFEEEEEE),onPrimary=Color(0xFF0A0A0A),secondary=Color(0xFFEEEEEE),onSecondary=Color(0xFF0A0A0A),background=Color(0xFF0A0A0A),surface=Color(0xFF0A0A0A),surfaceContainer=Color(0xFF161616),surfaceContainerLow=Color(0xFF111113),surfaceContainerHigh=Color(0xFF29292D),secondaryContainer=Color(0xFF161616),onSecondaryContainer=Color(0xFFE8E8EA),tertiaryContainer=Color(0xFF111113),onTertiaryContainer=Color(0xFFE8E8EA),onSurface=Color(0xFFE8E8EA),onSurfaceVariant=Color(0xFF9C9CA4),outline=Color(0xFF353538),outlineVariant=Color(0xFF2B2B2E))
private val Light=lightColorScheme(primary=Color(0xFF151515),onPrimary=Color.White,background=Color(0xFFFAFAFA),surface=Color(0xFFFAFAFA),surfaceContainer=Color(0xFFF0F0F2),surfaceContainerLow=Color(0xFFF5F5F6),surfaceContainerHigh=Color(0xFFE6E6E8),secondaryContainer=Color(0xFFEFEFF1),onSecondaryContainer=Color(0xFF171719),onSurface=Color(0xFF171719),onSurfaceVariant=Color(0xFF696972),outlineVariant=Color(0xFFE0E0E3))
private val PortalTypography=Typography(
    headlineSmall=TextStyle(fontSize=22.sp,lineHeight=28.sp,fontWeight=FontWeight.SemiBold,letterSpacing=(-0.5).sp),
    titleLarge=TextStyle(fontSize=22.sp,lineHeight=28.sp,fontWeight=FontWeight.SemiBold),
    titleMedium=TextStyle(fontSize=16.sp,lineHeight=24.sp,fontWeight=FontWeight.SemiBold),
    bodyLarge=TextStyle(fontSize=16.sp,lineHeight=24.sp),
    bodyMedium=TextStyle(fontSize=14.sp,lineHeight=21.sp),
    bodySmall=TextStyle(fontSize=13.sp,lineHeight=19.sp),
    labelLarge=TextStyle(fontSize=14.sp,lineHeight=20.sp,fontWeight=FontWeight.Medium),
    labelMedium=TextStyle(fontSize=13.sp,lineHeight=18.sp,fontWeight=FontWeight.Medium),
    labelSmall=TextStyle(fontSize=12.sp,lineHeight=17.sp),
)
@Composable fun PortalTheme(mode:String="dark",content:@Composable ()->Unit) {
    val dark=when(mode.lowercase()) { "dark"->true;"light"->false;else->isSystemInDarkTheme() }
    MaterialTheme(colorScheme=if(dark)Dark else Light,typography=PortalTypography,shapes=Shapes(small=RoundedCornerShape(8.dp),medium=RoundedCornerShape(12.dp),large=RoundedCornerShape(20.dp),extraLarge=RoundedCornerShape(24.dp)),content=content)
}

@Composable fun PortalSwitch(checked:Boolean,onCheckedChange:(Boolean)->Unit,modifier:Modifier=Modifier,enabled:Boolean=true) {
    Switch(checked,onCheckedChange,modifier,enabled=enabled,colors=SwitchDefaults.colors(
        uncheckedThumbColor=MaterialTheme.colorScheme.onSurfaceVariant,
        uncheckedTrackColor=MaterialTheme.colorScheme.surfaceContainerHigh,
        uncheckedBorderColor=Color.Transparent,
        disabledUncheckedThumbColor=MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha=0.4f),
        disabledUncheckedTrackColor=MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha=0.6f),
        disabledUncheckedBorderColor=Color.Transparent,
    ))
}
