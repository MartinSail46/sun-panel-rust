<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Separator } from '@/components/ui/separator'
import { get } from '@/api/system/about'
import logo from '@/assets/logo.svg'

interface Version {
  backend: string
  backendVersion: string
  sourceBaseline: string
}
const metadata = ref<Version>()
const loading = ref(false)
const failed = ref(false)
const frontendVersion = import.meta.env.VITE_APP_VERSION
const frontendBuild = import.meta.env.VITE_APP_BUILD
const text = { current: '当前维护版本', backend: '后端版本', frontend: '前端版本', build: '前端构建', baseline: '来源源码基线', source: '原项目与作者', author: '原作者', sourceNote: '本地维护版本基于 Sun-Panel 公开源码，使用 Rust 后端并提供免登录工作区。原项目的发行版、镜像和问题反馈不代表当前维护版本。', noUpdate: '当前维护版本暂未配置独立发布地址，请按本地项目文档更新和备份。', repository: '原项目源码（来源）', license: '保留原项目开源许可与作者信息。', error: '版本信息加载失败', retry: '重试' }
async function load() {
  if (loading.value)
    return
  loading.value = true
  failed.value = false
  try {
    const response = await get<Version>()
    if (response.code !== 0)
      throw new Error(response.msg)
    metadata.value = response.data
  }
  catch {
    failed.value = true
  }
  finally {
    loading.value = false
  }
}
onMounted(load)
</script>

<template>
  <div class="p-5">
    <div class="flex flex-col items-center gap-2">
      <img :src="logo" width="80" height="80" alt="Sun-Panel">
      <h1 class="text-2xl font-semibold">
        Sun-Panel Rust
      </h1>
      <Badge variant="secondary">
        {{ text.current }}
      </Badge>
    </div>
    <dl class="version-grid mt-6 text-sm">
      <dt>{{ text.backend }}</dt><dd>{{ metadata ? `${metadata.backend} ${metadata.backendVersion}` : '—' }}</dd>
      <dt>{{ text.frontend }}</dt><dd>{{ frontendVersion }}</dd>
      <dt>{{ text.build }}</dt><dd class="break-all">
        {{ frontendBuild }}
      </dd>
      <dt>{{ text.baseline }}</dt><dd>{{ metadata?.sourceBaseline || '—' }}</dd>
    </dl>
    <div v-if="failed" role="alert" class="flex items-center gap-2 mt-3 text-sm">
      <span>{{ text.error }}</span><Button variant="outline" size="sm" :disabled="loading" @click="load">
        {{ text.retry }}
      </Button>
    </div>
    <p class="mt-5 text-sm opacity-80">
      {{ text.noUpdate }}
    </p>
    <Separator class="my-5" /><h2 class="mb-3 font-semibold">
      {{ text.source }}
    </h2>
    <p class="text-sm leading-relaxed">
      {{ text.sourceNote }}
    </p>
    <p class="mt-4">
      {{ text.author }}：<a href="https://github.com/hslr-s" target="_blank" rel="noopener noreferrer" class="text-primary underline underline-offset-4">红烧猎人 / hslr-s</a>
    </p>
    <p class="mt-2">
      <a href="https://github.com/hslr-s/sun-panel" target="_blank" rel="noopener noreferrer" class="text-primary underline underline-offset-4">{{ text.repository }}</a>
    </p>
    <p class="mt-4 text-xs opacity-70">
      {{ text.license }}
    </p>
  </div>
</template>

<style scoped>
.version-grid { display:grid;grid-template-columns:auto 1fr;gap:10px 18px; }
.version-grid dt { opacity:.7; }
.version-grid dd { min-width:0;overflow-wrap:anywhere; }
</style>
