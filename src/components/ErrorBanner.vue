<template>
  <div v-if="gwError" class="gw-error">
    <span class="gw-error-msg">⚠ {{ gwError.message }}</span>
    <button class="mini-btn" @click="retryGw">一键重试</button>
    <button class="mini-btn" @click="dismissGwError">关闭</button>
  </div>
</template>

<script setup lang="ts">
import { gwError, dismissGwError } from '../composables/useGateway'
import { retryGw } from '../composables/useRecovery'
</script>
