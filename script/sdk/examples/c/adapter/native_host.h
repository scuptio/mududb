/* Native replacements for the guest's linear-memory host imports. */
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "mudu_sys.h"
#include "mududb/adapter/native.h"
extern mudu_native *wallet_adapter;
void *mudu_arena_alloc(size_t size, size_t align) { (void)align; return malloc(size); }
static void native_call(const char *operation,const uint8_t *input,uint32_t length,uint8_t *retptr) {
    mudu_native_buffer result={0};
    int status=mudu_native_call(wallet_adapter,operation,input,length,&result);
    if(status || result.len>UINT32_MAX) {
        fputs("Adapter call failed\n",stderr); mudu_native_release(wallet_adapter,result); exit(1);
    }
    uint8_t *copy=(uint8_t*)malloc(result.len);
    if(!copy && result.len) {mudu_native_release(wallet_adapter,result);exit(1);}
    memcpy(copy,result.data,result.len);
    uint32_t n=(uint32_t)result.len;
    mudu_native_release(wallet_adapter,result);
    memcpy(retptr,&copy,sizeof(copy));memcpy(retptr+sizeof(copy),&n,4);
}
static void mudu_host_query(const uint8_t *p,uint32_t n,uint8_t *r){native_call("query",p,n,r);}
static void mudu_host_command(const uint8_t *p,uint32_t n,uint8_t *r){native_call("command",p,n,r);}
