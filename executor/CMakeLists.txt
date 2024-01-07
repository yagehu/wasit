cmake_minimum_required(VERSION 3.22)
project(wazzi-executor)
enable_language(C)

include(FetchContent)

FetchContent_Declare(
  c-capnproto
  GIT_REPOSITORY https://gitlab.com/dkml/ext/c-capnproto.git
  GIT_TAG        f07596dbb516329d27ea95060a758f5d48d26366
  OVERRIDE_FIND_PACKAGE
)
find_package(c-capnproto)

install(
  TARGETS capnpc-c
  RUNTIME DESTINATION ${CMAKE_CURRENT_BINARY_DIR}
  COMPONENT capnpc-c
)

set(BUILD_TESTING OFF)

add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/wazzi-executor.capnp.c ${CMAKE_CURRENT_BINARY_DIR}/wazzi-executor.capnp.h
  COMMAND
    capnp compile
      -o c:${CMAKE_CURRENT_BINARY_DIR}
      --src-prefix=${CMAKE_CURRENT_SOURCE_DIR}
      ${CMAKE_CURRENT_SOURCE_DIR}/wazzi-executor.capnp
  MAIN_DEPENDENCY ${CMAKE_CURRENT_SOURCE_DIR}/wazzi-executor.capnp
)

add_executable(
  wazzi-executor
  main.c
  wazzi-executor.capnp.c
)
target_compile_options(
  wazzi-executor
  PRIVATE -Wall -Wextra -Wpedantic -Werror
)
set_target_properties(
  wazzi-executor
  PROPERTIES
  INCLUDE_DIRECTORIES ${CMAKE_CURRENT_BINARY_DIR}
)
target_link_libraries(wazzi-executor PRIVATE CapnC::Runtime)
