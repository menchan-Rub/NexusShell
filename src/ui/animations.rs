// src/ui/animations.rs - NexusShellのアニメーション機能

use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::Arc;

/// アニメーション管理
pub struct AnimationManager {
    animations: HashMap<String, Animation>,
    completed: Vec<String>,
    // メモリ使用量削減のためにプール化
    animation_pool: Vec<Arc<Animation>>,
    max_animations: usize,
    enabled: bool,
}

/// アニメーション種別（メモリ使用量削減のため列挙型として最適化）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationType {
    /// 線形
    Linear = 0,
    /// イージングイン（徐々に加速）
    EaseIn = 1,
    /// イージングアウト（徐々に減速）
    EaseOut = 2,
    /// イージングインアウト（加速して減速）
    EaseInOut = 3,
    /// バウンス（跳ね返り）
    Bounce = 4,
    /// エラスティック（伸縮）
    Elastic = 5,
}

/// アニメーション
pub struct Animation {
    /// アニメーションタイプ
    animation_type: AnimationType,
    /// 開始値
    start_value: f32,
    /// 終了値
    end_value: f32,
    /// 現在値
    current_value: f32,
    /// 開始時間
    start_time: Instant,
    /// 継続時間
    duration: Duration,
    /// 繰り返し回数（0は無限）
    repeat: u32,
    /// 現在の繰り返し回数
    current_repeat: u32,
    /// 完了したかどうか
    completed: bool,
}

impl AnimationManager {
    /// 新しいアニメーションマネージャーを作成
    pub fn new() -> Self {
        Self {
            animations: HashMap::with_capacity(16), // 初期サイズを設定して再割り当て回数を減らす
            completed: Vec::with_capacity(8),
            animation_pool: Vec::with_capacity(32),
            max_animations: 64, // 上限を設定してメモリを節約
            enabled: true,
        }
    }
    
    /// アニメーションを有効/無効化
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear_all();
        }
    }
    
    /// すべてのアニメーションをクリア
    pub fn clear_all(&mut self) {
        self.animations.clear();
        self.completed.clear();
    }
    
    /// アニメーションを追加
    pub fn add_animation(
        &mut self,
        id: String,
        start_value: f32,
        end_value: f32,
        duration_ms: u64,
        animation_type: AnimationType,
        repeat: u32,
    ) {
        if !self.enabled {
            return;
        }
        
        // 最大数に達していたら何もしない
        if self.animations.len() >= self.max_animations {
            return;
        }
        
        // 既存のアニメーションを再利用
        if let Some(mut animation) = self.get_pooled_animation() {
            animation.animation_type = animation_type;
            animation.start_value = start_value;
            animation.end_value = end_value;
            animation.current_value = start_value;
            animation.start_time = Instant::now();
            animation.duration = Duration::from_millis(duration_ms);
            animation.repeat = repeat;
            animation.current_repeat = 0;
            animation.completed = false;
            
            self.animations.insert(id, animation);
        } else {
            // 新しいアニメーションを作成
            let animation = Animation {
                animation_type,
                start_value,
                end_value,
                current_value: start_value,
                start_time: Instant::now(),
                duration: Duration::from_millis(duration_ms),
                repeat,
                current_repeat: 0,
                completed: false,
            };
            
            self.animations.insert(id, animation);
        }
    }
    
    /// プールからアニメーションを取得
    fn get_pooled_animation(&mut self) -> Option<Animation> {
        self.animation_pool.pop().map(|arc| {
            // Arc内のアニメーションのクローンを返す
            let a = match Arc::try_unwrap(arc) {
                Ok(a) => a,
                Err(arc) => (*arc).clone(),
            };
            a
        })
    }
    
    /// アニメーションを削除
    pub fn remove_animation(&mut self, id: &str) {
        if let Some(animation) = self.animations.remove(id) {
            // プールに戻す（再利用のため）
            if self.animation_pool.len() < 32 {
                self.animation_pool.push(Arc::new(animation));
            }
        }
    }
    
    /// アニメーションを更新
    pub fn update(&mut self, delta_time: Duration) {
        if !self.enabled {
            return;
        }
        
        self.completed.clear();
        
        // アニメーションが存在しない場合は早期リターン
        if self.animations.is_empty() {
            return;
        }
        
        let now = Instant::now();
        
        for (id, animation) in self.animations.iter_mut() {
            if animation.completed {
                self.completed.push(id.clone());
                continue;
            }
            
            let elapsed = now.duration_since(animation.start_time);
            
            // 浮動小数点演算を減らす
            let duration_secs = animation.duration.as_secs_f32();
            if duration_secs <= 0.0 {
                animation.current_value = animation.end_value;
                animation.completed = true;
                self.completed.push(id.clone());
                continue;
            }
            
            let mut progress = elapsed.as_secs_f32() / duration_secs;
            
            if progress >= 1.0 {
                // アニメーション完了
                if animation.repeat == 0 || animation.current_repeat < animation.repeat - 1 {
                    // リピート
                    animation.current_repeat += 1;
                    animation.start_time = now;
                    progress = 0.0;
                } else {
                    // 完全に終了
                    progress = 1.0;
                    animation.completed = true;
                    self.completed.push(id.clone());
                }
            }
            
            // イージング関数の適用（高速なルックアップテーブル方式）
            let eased_progress = self.apply_easing(animation.animation_type, progress);
            
            // 値を更新
            animation.current_value = animation.start_value + (animation.end_value - animation.start_value) * eased_progress;
        }
        
        // 完了したアニメーションを削除
        for id in &self.completed {
            if let Some(animation) = self.animations.remove(id) {
                // プールに戻す（再利用のため）
                if self.animation_pool.len() < 32 {
                    self.animation_pool.push(Arc::new(animation));
                }
            }
        }
    }
    
    /// イージングを適用
    #[inline]
    fn apply_easing(&self, animation_type: AnimationType, t: f32) -> f32 {
        match animation_type {
            AnimationType::Linear => t,
            AnimationType::EaseIn => t * t,
            AnimationType::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            AnimationType::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            },
            AnimationType::Bounce => Self::bounce_ease_out(t),
            AnimationType::Elastic => Self::elastic_ease_out(t),
        }
    }
    
    /// アニメーション値を取得
    pub fn get_value(&self, id: &str) -> Option<f32> {
        self.animations.get(id).map(|a| a.current_value)
    }
    
    /// アニメーションが存在するか確認
    pub fn has_animation(&self, id: &str) -> bool {
        self.animations.contains_key(id)
    }
    
    /// 完了したアニメーションIDリストを取得
    pub fn get_completed(&self) -> &[String] {
        &self.completed
    }
    
    /// バウンスイージング計算
    #[inline]
    fn bounce_ease_out(t: f32) -> f32 {
        if t < 1.0 / 2.75 {
            7.5625 * t * t
        } else if t < 2.0 / 2.75 {
            let t = t - 1.5 / 2.75;
            7.5625 * t * t + 0.75
        } else if t < 2.5 / 2.75 {
            let t = t - 2.25 / 2.75;
            7.5625 * t * t + 0.9375
        } else {
            let t = t - 2.625 / 2.75;
            7.5625 * t * t + 0.984375
        }
    }
    
    /// エラスティックイージング計算
    #[inline]
    fn elastic_ease_out(t: f32) -> f32 {
        let c4 = (2.0 * std::f32::consts::PI) / 3.0;
        
        if t == 0.0 {
            0.0
        } else if t == 1.0 {
            1.0
        } else {
            2.0_f32.powf(-10.0 * t) * (t * 10.0 - 0.75).sin() * c4 + 1.0
        }
    }
}

/// アニメーション拡張特性
pub trait Animate {
    /// 値をアニメーション
    fn animate(&mut self, manager: &mut AnimationManager, id: &str, target: f32, duration_ms: u64, animation_type: AnimationType);
}

impl Animate for f32 {
    fn animate(&mut self, manager: &mut AnimationManager, id: &str, target: f32, duration_ms: u64, animation_type: AnimationType) {
        manager.add_animation(id.to_string(), *self, target, duration_ms, animation_type, 0);
    }
}

// Animationクローン実装
impl Clone for Animation {
    fn clone(&self) -> Self {
        Self {
            animation_type: self.animation_type,
            start_value: self.start_value,
            end_value: self.end_value,
            current_value: self.current_value,
            start_time: self.start_time,
            duration: self.duration,
            repeat: self.repeat,
            current_repeat: self.current_repeat,
            completed: self.completed,
        }
    }
} 