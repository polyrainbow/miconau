/*
Copyright 2023 Zachary Churchill

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the “Software”), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

use std::marker::PhantomData;
use std::time::Duration;

use rodio::{Sample, Source};

/// An empty source.
pub struct Callback<S> {
    pub phantom_data: PhantomData<S>,
    pub callback: Box<dyn Send + Fn()>,
}

impl<S> Callback<S> {
    #[inline]
    pub fn new(callback: Box<dyn Send + Fn()>) -> Callback<S> {
        Callback {
            phantom_data: PhantomData,
            callback,
        }
    }
}

impl<S> Iterator for Callback<S> {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        (self.callback)();
        None
    }
}

impl<S> Source for Callback<S>
where
    S: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        48000
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }
}
