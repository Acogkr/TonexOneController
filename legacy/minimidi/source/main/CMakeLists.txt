set(COMPONENT_SRCS "minimidi.c")
set(COMPONENT_ADD_INCLUDEDIRS ".")

register_component()
target_compile_options(${COMPONENT_LIB} PRIVATE -Werror=unused-variable)