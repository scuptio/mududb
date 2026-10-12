#include <stdio.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include "mudu_sys.h"
#include "mududb/adapter/native.h"
#include "mududb/types/UniSyscall.h"
mudu_native *wallet_adapter;
int create_user(const mudu_proc_param*,mudu_datum*,mudu_error*);
int deposit(const mudu_proc_param*,mudu_datum*,mudu_error*);
int withdraw(const mudu_proc_param*,mudu_datum*,mudu_error*);
int transfer_funds(const mudu_proc_param*,mudu_datum*,mudu_error*);
int balance(const mudu_proc_param*,mudu_datum*,mudu_error*);
static mudu_oid session;
static int64_t integer(const char *text) {
    char *end; errno=0;
    long long value=strtoll(text,&end,10);
    if(errno || end==text || *end){fputs("Expected a signed 64-bit integer\n",stderr);exit(1);}
    return (int64_t)value;
}

static int execute(mudu_proc_fn function,mudu_datum *args,uint32_t count,int64_t *value) {
    mudu_proc_param param={0};param.session=session;param.params=args;param.n_params=count;
    mudu_datum result; mudu_error error={0};
    if(function(&param,&result,&error)){fprintf(stderr,"%.*s\n",(int)error.msg_len,error.msg);return 1;}
    *value=result.i64;return 0;
}
static int initialize(void){
    const char *ddl[]={"CREATE TABLE users (user_id BIGINT PRIMARY KEY, name VARCHAR(100), email VARCHAR(100), created_at BIGINT, updated_at BIGINT)","CREATE TABLE wallets (user_id BIGINT PRIMARY KEY, balance BIGINT, updated_at BIGINT)"};
    for(size_t i=0;i<2;i++){uint64_t n; mudu_error e={0};if(mudu_command(session,ddl[i],NULL,0,&n,&e)){fprintf(stderr,"%.*s\n",(int)e.msg_len,e.msg);return 1;}}return 0;
}
static int invoke_cli(int argc,char **argv){
    if(argc<2)return 1;
    if(!strcmp(argv[1],"init"))return initialize();
    mudu_datum args[3];int64_t value=0;
    if(!strcmp(argv[1],"scenario")){
        args[0]=mudu_i64(1001);args[1]=mudu_str("Alice",5);args[2]=mudu_str("alice@example.com",17);
        if(execute(create_user,args,3,&value))return 1;
        args[0]=mudu_i64(1002);args[1]=mudu_str("Bob",3);args[2]=mudu_str("bob@example.com",15);
        if(execute(create_user,args,3,&value))return 1;
        args[0]=mudu_i64(1001);args[1]=mudu_i64(1000);if(execute(deposit,args,2,&value))return 1;
        args[1]=mudu_i64(100);if(execute(withdraw,args,2,&value))return 1;
        args[1]=mudu_i64(1002);args[2]=mudu_i64(250);if(execute(transfer_funds,args,3,&value))return 1;
        if(execute(balance,args,1,&value)||value!=650)return 1;
        args[0]=mudu_i64(1002);if(execute(balance,args,1,&value)||value!=250)return 1;
        puts("{\"alice\":650,\"bob\":250}");return 0;
    }
    if(argc<3)return 1;
    args[0]=mudu_i64(integer(argv[2]));
    mudu_proc_fn fn=NULL;uint32_t count=1;
    if(!strcmp(argv[1],"balance")&&argc==3)fn=balance;
    if(!strcmp(argv[1],"create")&&argc==5){fn=create_user;count=3;args[1]=mudu_str(argv[3],(uint32_t)strlen(argv[3]));args[2]=mudu_str(argv[4],(uint32_t)strlen(argv[4]));}
    if(argc==4){count=2;args[1]=mudu_i64(integer(argv[3]));if(!strcmp(argv[1],"deposit"))fn=deposit;if(!strcmp(argv[1],"withdraw"))fn=withdraw;}
    if(!strcmp(argv[1],"transfer")&&argc==5){fn=transfer_funds;count=3;args[1]=mudu_i64(integer(argv[3]));args[2]=mudu_i64(integer(argv[4]));}
    if(!fn)return 1;
    if(execute(fn,args,count,&value))return 1;
    printf("%lld\n",(long long)value);return 0;
}
int main(int argc,char **argv){
    wallet_adapter=mudu_native_load(getenv("MUDU_ADAPTER_LIBRARY"));
    if(!wallet_adapter){fputs("Set MUDU_ADAPTER_LIBRARY to a matching ABI v1 runtime\n",stderr);return 1;}
    mp_writer w;mpw_init(&w);uni_oid worker={0};uni_syscall_open_session_request_encode(&w,&worker);
    mudu_native_buffer result={0};int status=mudu_native_call(wallet_adapter,"open",w.buf,w.len,&result);mpw_free(&w);
    if(status){mudu_native_release(wallet_adapter,result);mudu_native_unload(wallet_adapter);return 1;}
    mp_reader r;mp_arena a; mpr_init(&r,result.data,result.len);mpa_init(&a);uni_syscall_open_session_result opened;
    if(uni_syscall_open_session_result_decode(&r,&a,&opened)||opened.is_err){mudu_native_release(wallet_adapter,result);mudu_native_unload(wallet_adapter);return 1;}
    session=opened.value;mudu_native_release(wallet_adapter,result);
    status=invoke_cli(argc,argv);
    mpw_init(&w);uni_syscall_close_session_request_encode(&w,&session);
    int closed=mudu_native_call(wallet_adapter,"close",w.buf,w.len,&result);mpw_free(&w);mudu_native_release(wallet_adapter,result);mudu_native_unload(wallet_adapter);
    return status||closed;
}
