<script setup lang="ts">
import 'vue-sonner/style.css'
import FeedbackHost from '@/components/common/FeedbackHost.vue'
import { useTheme } from '@/hooks/useTheme'
const { isDark } = useTheme()
</script>
<template>
  <RouterView />
  <FeedbackHost :dark="isDark" />
</template>
