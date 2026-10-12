/* Node-API bridge for the ABI v1 adapter; built into a separate .node addon. */
#include <node_api.h>
#include "native.h"
#define CHECK(expr) do { if ((expr) != napi_ok) { napi_throw_error(env, NULL, "Invalid adapter bridge argument"); return NULL; } } while (0)
static void finalize(napi_env env, void *data, void *hint) { (void)env; (void)hint; mudu_native_unload((mudu_native*)data); }
static napi_value open_adapter(napi_env env, napi_callback_info info) {
    napi_value args[1], result; size_t argc=1, length=0;
    CHECK(napi_get_cb_info(env,info,&argc,args,NULL,NULL));
    if(argc != 1) { napi_throw_error(env,NULL,"open requires a library path"); return NULL; }
    CHECK(napi_get_value_string_utf8(env,args[0],NULL,0,&length));
    char *path=(char*)malloc(length+1);
    if(!path) { napi_throw_error(env,NULL,"Out of memory"); return NULL; }
    if(napi_get_value_string_utf8(env,args[0],path,length+1,&length)!=napi_ok) { free(path); napi_throw_error(env,NULL,"Invalid library path"); return NULL; }
    mudu_native *client=mudu_native_load(path); free(path);
    if(!client) { napi_throw_error(env,NULL,"Cannot load adapter: check path, OS, architecture, dependencies and ABI v1"); return NULL; }
    if(napi_create_external(env,client,finalize,NULL,&result)!=napi_ok) { mudu_native_unload(client); napi_throw_error(env,NULL,"Cannot retain adapter"); return NULL; }
    return result;
}
static napi_value call_adapter(napi_env env, napi_callback_info info) {
    napi_value args[3], result; size_t argc=3, length=0; void *data=NULL; mudu_native *client=NULL; char operation[64];
    CHECK(napi_get_cb_info(env,info,&argc,args,NULL,NULL));
    if(argc != 3) { napi_throw_error(env,NULL,"call requires handle, operation and Buffer"); return NULL; }
    CHECK(napi_get_value_external(env,args[0],(void**)&client));
    CHECK(napi_get_value_string_utf8(env,args[1],NULL,0,&length));
    if(!client || length>=sizeof(operation)) { napi_throw_error(env,NULL,"Invalid operation"); return NULL; }
    CHECK(napi_get_value_string_utf8(env,args[1],operation,sizeof(operation),&length));
    CHECK(napi_get_buffer_info(env,args[2],&data,&length));
    mudu_native_buffer output={0};
    int status=mudu_native_call(client,operation,(const uint8_t*)data,length,&output);
    if(status) {
        napi_value message,error;
        napi_status ns=napi_create_string_utf8(env,(const char*)output.data,output.len,&message);
        mudu_native_release(client,output);
        CHECK(ns); CHECK(napi_create_error(env,NULL,message,&error)); CHECK(napi_throw(env,error)); return NULL;
    }
    napi_status ns=napi_create_buffer_copy(env,output.len,output.data,NULL,&result);
    mudu_native_release(client,output); CHECK(ns); return result;
}
NAPI_MODULE_INIT() {
    napi_value open,call;
    CHECK(napi_create_function(env,"open",NAPI_AUTO_LENGTH,open_adapter,NULL,&open));
    CHECK(napi_create_function(env,"call",NAPI_AUTO_LENGTH,call_adapter,NULL,&call));
    CHECK(napi_set_named_property(env,exports,"open",open)); CHECK(napi_set_named_property(env,exports,"call",call)); return exports;
}
