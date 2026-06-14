// Simple test script to insert batch data
import { invoke } from '@tauri-apps/api/core';

async function main() {
  console.log('Inserting 10000 test items...');
  const startTime = Date.now();
  
  try {
    const result = await invoke('test_insert_batch', { count: 10000 });
    const elapsed = Date.now() - startTime;
    console.log(`Successfully inserted ${result} items in ${elapsed}ms`);
  } catch (error) {
    console.error('Failed to insert test data:', error);
  }
}

main();
