#define _POSIX_C_SOURCE 200809L
#include <ffi.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

/* Loaded libraries live until process exit. Calls are serialized, including
   string copying, so native static return buffers remain valid during copying. */
typedef struct Library { char *path; void *handle; struct Library *next; } Library;
static Library *libraries;
static pthread_mutex_t gate = PTHREAD_MUTEX_INITIALIZER;
typedef union { int64_t integer; uint8_t boolean; const char *text; ffi_arg word; } Slot;
static ffi_type *kind_type(uint8_t kind) {
    switch(kind) {case 0:return &ffi_type_void;case 1:return &ffi_type_sint64;
        case 2:return &ffi_type_uint8;case 3:return &ffi_type_pointer;default:return NULL;}
}
int lia_ffi_call(const char *path,const char *symbol,size_t count,
    const uint8_t *kinds,const int64_t *numbers,const char *const *strings,
    uint8_t result_kind,int64_t *number,char **text,char *error,size_t capacity) {
    int status=-1; const char *failure="foreign call failed";
    ffi_type **types=NULL; void **args=NULL; Slot *slots=NULL; Slot result={0};
    pthread_mutex_lock(&gate);
    const char *key=path?path:"";
    Library *library=libraries;
    while(library && strcmp(library->path,key)) library=library->next;
    if(!library) {
        void *handle=dlopen(path,RTLD_NOW|RTLD_LOCAL);
        if(!handle){failure=dlerror();goto done;}
        library=calloc(1,sizeof(*library));
        if(!library){dlclose(handle);failure="out of memory";goto done;}
        library->path=strdup(key);
        if(!library->path){free(library);dlclose(handle);failure="out of memory";goto done;}
        library->handle=handle;library->next=libraries;libraries=library;
    }
    dlerror();
    void *function=dlsym(library->handle,symbol);
    const char *lookup_error=dlerror();
    if(lookup_error || !function){failure=lookup_error?lookup_error:"null foreign symbol";goto done;}
    types=calloc(count?count:1,sizeof(*types));args=calloc(count?count:1,sizeof(*args));slots=calloc(count?count:1,sizeof(*slots));
    if(!types || !args || !slots){failure="out of memory";goto done;}
    for(size_t i=0;i<count;i++) {
        types[i]=kind_type(kinds[i]);
        switch(kinds[i]){
            case 1:slots[i].integer=numbers[i];args[i]=&slots[i].integer;break;
            case 2:slots[i].boolean=(uint8_t)numbers[i];args[i]=&slots[i].boolean;break;
            case 3:slots[i].text=strings[i];args[i]=&slots[i].text;break;
            default:failure="invalid foreign argument kind";goto done;
        }
    }
    ffi_type *return_type=kind_type(result_kind);
    if(!return_type){failure="invalid foreign return kind";goto done;}
    ffi_cif cif;
    if(ffi_prep_cif(&cif,FFI_DEFAULT_ABI,(unsigned int)count,return_type,types)!=FFI_OK){failure="unsupported C signature";goto done;}
    ffi_call(&cif,FFI_FN(function),&result,args);
    switch(result_kind){
        case 0:break;
        case 1:*number=result.integer;break;
        case 2:*number=(int64_t)result.word;break;
        case 3:
            if(!result.text){failure="native function returned a null String";goto done;}
            *text=strdup(result.text);
            if(!*text){failure="out of memory";goto done;}
            break;
    }
    status=0;
done:
    if(status && capacity)snprintf(error,capacity,"%s",failure?failure:"native loader error");
    free(types);free(args);free(slots);pthread_mutex_unlock(&gate);return status;
}
