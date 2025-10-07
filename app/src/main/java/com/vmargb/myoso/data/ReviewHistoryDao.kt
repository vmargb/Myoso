package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface ReviewHistoryDao {
    
    @Query("SELECT * FROM review_history WHERE cardId = :cardId ORDER BY reviewedAt DESC")
    suspend fun getReviewHistoryByCard(cardId: String): List<ReviewHistoryEntity>
    
    @Query("SELECT * FROM review_history WHERE cardId = :cardId ORDER BY reviewedAt DESC")
    fun getReviewHistoryByCardFlow(cardId: String): Flow<List<ReviewHistoryEntity>>
    
    @Query("SELECT * FROM review_history ORDER BY reviewedAt DESC LIMIT :limit")
    suspend fun getRecentReviews(limit: Int = 100): List<ReviewHistoryEntity>
    
    @Query("SELECT * FROM review_history WHERE reviewedAt >= :startTime AND reviewedAt <= :endTime")
    suspend fun getReviewsInTimeRange(startTime: Long, endTime: Long): List<ReviewHistoryEntity>
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertReviewHistory(review: ReviewHistoryEntity)
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertReviewHistories(reviews: List<ReviewHistoryEntity>)
    
    @Delete
    suspend fun deleteReviewHistory(review: ReviewHistoryEntity)
    
    @Query("DELETE FROM review_history WHERE cardId = :cardId")
    suspend fun deleteReviewHistoryByCard(cardId: String)
    
    @Query("SELECT COUNT(*) FROM review_history WHERE cardId = :cardId")
    suspend fun getReviewCountByCard(cardId: String): Int
    
    @Query("SELECT AVG(responseTimeMs) FROM review_history WHERE cardId = :cardId")
    suspend fun getAverageResponseTimeByCard(cardId: String): Double?
    
    @Query("DELETE FROM review_history")
    suspend fun deleteAllReviewHistory()
}
