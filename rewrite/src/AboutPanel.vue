<script setup lang="ts">
import {inject,computed} from 'vue';
import {invoke} from '@tauri-apps/api/core';
const emit=defineEmits<{notice:[message:string]}>();
async function openLink(kind:'privacy'|'terms'|'license'|'repository'){try{await invoke('open_product_link',{kind,locale:languages?.snapshot.locale||'zh-CN'});}catch(error){emit('notice',(error as {message?:string})?.message||String(error));}}
import {languageKey,type Languages} from './baseline-language';
const languages=inject<Languages|null>(languageKey,null);
const legalFile=(name:string)=>{const locale=languages?.snapshot.locale||'zh-CN',suffix=({'zh-Hant':'zh-Hant','en-US':'en','fr-FR':'fr','ru-RU':'ru','ja-JP':'ja','es-ES':'es','th-TH':'th'} as Record<string,string>)[locale];return suffix?`docs/legal/${name}.${suffix}.md`:`${name}.md`;};
const privacy=computed(()=>legalFile('PRIVACY')),terms=computed(()=>legalFile('TERMS'));
import logo from './assets/ITM_logo_tiny.png';
import UpdatePanel from './UpdatePanel.vue';
</script>
<template><section class="aboutCard" aria-label="关于 IMDb Tech Manager"><div class="aboutHead"><img :src="logo" alt=""><span>关于</span></div><h3 class="aboutTitle">IMDb Tech Manager</h3><p class="aboutSummary">NFO 技术规格、标签与批量任务管理工具</p><p class="aboutMeta">v5.0.0</p><div class="aboutRule"></div><UpdatePanel /><div class="aboutRule"></div><p class="aboutDisclaimer">本软件为独立开发工具，与 IMDb.com, Inc. 或 tinyMediaManager 无隶属、授权或背书关系。相关商标归各自权利人所有。</p><div class="aboutRule"></div><p class="aboutCredits">作者 侯雁泽　　© 2026 侯雁泽　　Apache License 2.0</p><nav class="aboutLinks"><a id="privacyLink" @click.prevent="openLink('privacy')" :href="'https://github.com/Eric-Hou1997/IMDb-Tech-Manager/blob/main/'+privacy" target="_blank" rel="noopener">隐私政策</a><a id="termsLink" @click.prevent="openLink('terms')" :href="'https://github.com/Eric-Hou1997/IMDb-Tech-Manager/blob/main/'+terms" target="_blank" rel="noopener">使用条款</a><a @click.prevent="openLink('license')" href="https://github.com/Eric-Hou1997/IMDb-Tech-Manager/blob/main/LICENSE" target="_blank" rel="noopener">开源许可</a><a @click.prevent="openLink('repository')" href="https://github.com/Eric-Hou1997/IMDb-Tech-Manager" target="_blank" rel="noopener">GitHub 项目</a></nav></section></template>
