package com.vmargb.myoso.data

import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import android.content.Context

@Database(
    entities = [
        CardEntity::class,
        DeckEntity::class,
        NoteEntity::class,
        CitationEntity::class,
        ReviewHistoryEntity::class
    ],
    version = 1,
    exportSchema = false
)
@TypeConverters(Converters::class)
abstract class AppDatabase : RoomDatabase() {
    
    abstract fun cardDao(): CardDao
    abstract fun deckDao(): DeckDao
    abstract fun noteDao(): NoteDao
    abstract fun reviewHistoryDao(): ReviewHistoryDao
    
    companion object {
        @Volatile
        private var INSTANCE: AppDatabase? = null
        
        fun getDatabase(context: Context): AppDatabase {
            return INSTANCE ?: synchronized(this) {
                val instance = Room.databaseBuilder(
                    context.applicationContext,
                    AppDatabase::class.java,
                    "myoso_database"
                ).build()
                INSTANCE = instance
                instance
            }
        }
    }
}
