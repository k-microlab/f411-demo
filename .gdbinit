# **NEW** Define a post remote hook so these commands are run after GDB connects to openocd otherwise these commands will get run too soon
define target hookpost-remote

# **REMOVED** CLion will already do this and will conflict
#target extended-remote :3333

# print demangled symbols
set print asm-demangle on

# set backtrace limit to not have infinite backtrace loops
set backtrace limit 32

# detect unhandled exceptions, hard faults and panics
break DefaultHandler
break HardFault
break rust_begin_unwind

# *try* to stop at the user entry point (it might be gone due to inlining)
break main

monitor arm semihosting enable

# Program the target with the application
load

# **REMOVED** Don't need this when debugging in CLion
#stepi

# **NEW** End the hook function
end