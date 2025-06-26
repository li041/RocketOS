#import "../components/cover.typ": *
#import "../components/figure.typ": *

= 信号处理模块
#h(2em)信号是操作系统向进程传递事件通知的一种机制，主要用来通知进程某个特定事件的发生，或者是让进程执行某个特定的处理函数。

== 信号结构

#h(2em)Sig结构是信号在内核中的基本承载体，设计简洁高效，专门用于内核内部的信号管理和处理。它包含了信号识别、分发和处理过程中的核心信息，去除了冗余字段，确保在信号处理的关键路径上保持最佳性能。

#h(2em)SigInfo结构则为信号传递提供了更丰富的信息描述。在RocketOS中，SigInfo包含了信号处理中最常用的关键字段：signo标识具体信号类型，code提供信号产生的原因和上下文，fields根据不同信号类型承载相应的附加数据。这种精简设计既满足了大部分信号处理需求，又避免了结构过于庞大影响系统性能。

#h(2em)当涉及用户态和内核态之间的信号信息传递时，RocketOS采用LinuxSigInfo结构作为标准化接口。LinuxSigInfo严格遵循POSIX标准规范，包含标准要求的所有字段和数据布局，确保与现有Linux应用程序的完全兼容。这种设计不仅体现在数据结构字段对应上，还包括内存布局、字节对齐以及各种信号类型特定信息的组织方式

#code-figure(
    ```rs
    pub struct SigInfo {
      pub signo: i32,      // 信号值
      pub code: i32,       // 信号产生原因
      pub fields: SiField, // 额外信息
    }
  ```,
  caption: [信号信息结构体SigInfo],
  label-name: "信号信息结构体",
)

#h(2em)通过内核内部使用精简Sig和SigInfo结构，而在系统调用接口使用标准LinuxSigInfo结构的双重设计，RocketOS实现了内核效率与标准兼容性的平衡，既保证了内核高效运行，又确保了应用程序的无缝迁移和正确执行。

== 信号发送

#h(2em)对于线程级信号，也就是只针对特定线程的信号，系统会直接操作目标任务的信号待处理队列。通过op_sig_pending_mut方法获取任务的信号待处理状态的可变引用，然后调用add_signal方法将新的信号信息添加到待处理队列中。这个过程确保了信号不会丢失，即使当前任务正在执行其他操作。添加信号后，系统会检查当前任务是否满足可中断条件，这个检查通过check_interrupt方法完成，该方法会考虑任务当前的状态、信号屏蔽字以及其他相关因素来决定是否应该立即处理信号。

#h(2em)对于进程级信号，处理过程更加复杂，因为这类信号需要影响整个进程组中的所有线程。系统会通过op_thread_group_mut方法获取进程组的可变引用，然后遍历进程组中的每一个任务。对于每个任务，系统都会执行与线程级信号相同的操作序列：将信号添加到任务的待处理队列中，检查中断条件，如果满足条件则设置中断标志、从等待队列中移除、设置为就绪状态并加入调度队列。这种设计确保了进程级信号能够被进程组中的任何一个线程处理，提高了信号处理的灵活性和效率。

#h(2em)如果任务满足中断条件，系统会执行一系列操作来确保信号能够被及时处理。首先，系统会在信号待处理状态中设置中断标志，通过set_interrupted方法标记任务已被信号中断。然后调用delete_wait函数将任务从等待队列中移除，这是因为如果任务正在等待某个事件（比如I/O操作完成或者锁的释放），信号的到达应该能够中断这种等待状态。接着，系统会调用set_ready方法将任务状态设置为就绪，表示任务可以被调度器选中执行。最后，通过add_task方法将任务重新加入到调度器的就绪队列中，确保任务能够尽快获得CPU时间来处理信号。

#algorithm-figure(
    ```rs
  输入：
  - task：当前接收信号的任务（线程）
  - siginfo：信号信息（包含 signo 等）
  - thread_level：布尔值，表示信号是线程级（true）还是进程级（false）

  输出：无

  2:  if thread_level == true then  # 线程级信号
  3:      task.sig_pending.add_signal(siginfo)
  4:      if task.check_interrupt() == true then
  5:          task.sig_pending.set_interrupted()
  6:          delete_wait(task.tid)
  7:          task.state ← READY
  8:          add_task(task)
        end if

  9:  else  # 进程级信号
  10:     for t ∈ task.thread_group do
  11:         t.sig_pending.add_signal(siginfo)
  12:         if t.check_interrupt() == true then
  13:             t.sig_pending.set_interrupted()
  14:             delete_wait(t.tid)
  15:             t.state ← READY
  16:             add_task(t)
            end if
         end for
    ```,
    caption: [接收信号算法],
    label-name: "receive-signal",
  )


== 信号处理流程
  === 基本信号处理流程
  #h(2em)信号的处理流程是操作系统中一个精密的异步事件处理机制。当系统事件发生时，内核首先在目标进程的进程控制块中设置相应的信号标志位，将信号标记为待处理状态。信号不会立即处理，而是等待特定时机，主要是进程从内核态返回用户态时、被唤醒时或系统调用返回时。

  #h(2em)内核检查待处理信号时，会先查看进程的信号屏蔽字，确定哪些信号被阻塞。对于未阻塞的信号，根据处理方式分别执行：默认处理直接执行系统预定义操作；忽略信号则清除待处理标志；自定义处理最为复杂。

  #h(2em)执行自定义信号处理时，内核先保存进程当前的执行上下文，在用户栈上构造特殊栈帧，修改程序计数器指向处理函数，并传递信号参数。进程在特殊环境中执行处理函数，期间通常会自动阻塞同类型信号防止重入。处理函数应保持简单快速，避免调用不安全函数。

  #h(2em)函数返回后，控制权转移到内核预设的返回代码，触发系统调用通知处理完成。内核随后执行清理工作，恢复保存的执行上下文、清理栈帧、恢复信号屏蔽字，最终将进程执行流程恢复到信号发生前的状态，确保进程继续正常执行。

  #figure(
  image("img/信号处理流程.drawio.png", width: 60%),
    caption: [信号处理流程图]
  ) <custom_signal_chart>

  

=== 对SA_RESTART的特别处理
  #h(2em) SA_RESTART是Linux信号处理机制中一个重要的标志位，它主要用于解决信号与系统调用之间的交互问题，确保系统调用的连续性和可靠性。

  #h(2em) 在linux中，SA_RESTART标志位实现是通过先进行重置再进行撤销而实现的，尽管能这样的做法能够保障信号处理的正确性，但相对的需要做一些浪费的保存操作，但在RocketOS中，我们通过对条件的判断来对是否进行重置操作的判断，判断的条件共需四条
   #pad(left: 4em)[
  + 需要重启的系统调用是否可以重启
  + 信号处理函数是否存在SA_RESTART标志位
  + 信号处理函数是否被被用户注册
  + 任务是否被信号中断阻塞
   ]

   #code-figure(
    ```rs
      if task.can_restart()
          && action.flags.contains(SigActionFlag::SA_RESTART)
          && task.is_interrupted() && action.sa_handler != SIG_DFL && action.sa_handler != SIG_IGN
      {
          // 回到用户调用ecall的指令
          log::warn!("[handle_signal] handle SA_RESTART");
          trap_cx.set_sepc(trap_cx.sepc - 4);
          trap_cx.restore_a0(); // 从last_a0中恢复a0
      }
    ```,
    caption: [SA_RESTART判断条件],
    label-name: "SA_RESTART判断条件",
  )

=== sigframe结构设计
  #h(2em) 在用户态进程的信号处理机制中，SA_SIGINFO标志位扮演着关键的角色，它决定了内核在信号处理过程中采用何种策略来构造和管理信号上下文信息。这个标志位的存在与否直接影响着信号处理函数的调用方式、参数传递机制以及上下文保存恢复的具体实现。当用户态进程通过sigaction系统调用注册自定义信号处理函数时，如果在sa_flags字段中设置了SA_SIGINFO标志位，这向内核表明该信号处理函数需要接收详细的信号信息。内核会根据这个标志位的存在来选择合适的信号帧结构体类型，确保为信号处理提供正确的上下文环境和参数传递机制。

  #h(2em) 对于未设置SA_SIGINFO标志位的情况，内核采用传统的信号处理方式。在这种模式下，内核会直接调用用户态的信号处理函数，仅将信号编号作为单一参数传递给处理函数。这种简化的处理方式对应着较为精简的上下文保存需求，因此内核选择构造常规的SigFrame结构体来满足这一需求。SigFrame结构体设计简洁明了，包含了一个FrameFlags类型的flag字段用于标识结构体类型，以及一个SigContext结构体用于保存处理器的执行上下文信息。

  #code-figure(
    ```rs
    pub struct SigFrame {
        pub flag: FrameFlags,       // 标志位
        pub sigcontext: SigContext, // 上下文信息
    }
      pub struct SigContext {
          pub sepc: usize,
          pub x: [usize; 32],
          pub last_a0: usize,
          pub kernel_tp: usize,
          pub mask: SigSet, // 记录原先的mask
      }
    ```,
    caption: [SigContext结构],
    label-name: "SigContext结构",
  )

  #h(2em) 当用户信号处理程序注册时设置了SA_SIGINFO标志位，情况变得更加复杂和功能丰富。在这种情况下，信号处理函数不仅需要接收信号编号，还需要获得包含详细信号信息的siginfo结构体以及完整的用户上下文信息。为了满足这些额外的需求，内核必须构造更为完整和复杂的SigRTFrame结构体。SigRTFrame结构体代表了信号处理机制的高级形态，它不仅包含了用于标识结构类型的flag字段，还整合了UContext结构体来提供完整的用户上下文信息，以及LinuxSigInfo结构体来传递详细的信号相关信息。这种设计使得信号处理函数能够获得更丰富的上下文信息，从而实现更精细化的信号处理逻辑。

  #code-figure(
    ```rs
    pub struct SigRTFrame {
        pub flag: FrameFlags,      // 标志位
        pub ucontext: UContext,    // 上下文信息
        pub siginfo: LinuxSigInfo, // 信号信息
    }
    pub struct UContext {
        pub uc_flags: usize,
        pub uc_link: usize,
        pub uc_stack: SignalStack,
        pub uc_sigmask: SigSet,
        pub uc_sig: [usize; 16],
        pub uc_mcontext: SigContext,
    }
    ```,
    caption: [SigContext结构],
    label-name: "SigContext结构",
  )

  #figure(
    image("img/sigframe结构图.drawio.png", width: 50%),
    caption: [sigframe结构图]
  ) <custom_signal_chart>

  #h(2em) 在用户信号处理流程结束后，会自动通过预先设置的跳板平台来调用sigreturn返回内核态，在sigreturn中通过在信号处理完成后检查sigframe中的flag字段，内核能够准确识别当前使用的结构体类型，从而选择相应的恢复策略来提取正确的上下文信息。这种设计不仅保证了不同信号处理模式下的功能正确性，还优化了系统资源的使用效率，避免了在简单信号处理场景中构造过于复杂的上下文结构。

  #figure(
      image("img/用户信号流程图.drawio.png", width: 80%),
      caption: [用户态与内核态交互流程图]
  ) <custom_signal_chart>

