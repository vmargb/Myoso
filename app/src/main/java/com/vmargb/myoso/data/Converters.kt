package com.vmargb.myoso.data

import androidx.room.TypeConverter

// ==========================================
// Converters for complex data types
// ==========================================
/**
 * converts complex data types for Room database storage
 * room does not natively support lists or arrays
 * so the converter transforms List<String> to a CSV string and vice versa
 */
class Converters {
    
    @TypeConverter
    fun fromStringList(value: List<String>?): String? { // string list to CSV
        return value?.joinToString(",")
    }

    @TypeConverter
    fun toStringList(value: String?): List<String>? { // CSV to string list
        return value?.split(",")?.filter { it.isNotBlank() } // filter out empty strings
    }
}
