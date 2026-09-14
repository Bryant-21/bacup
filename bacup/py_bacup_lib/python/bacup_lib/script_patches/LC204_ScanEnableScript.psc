Function RunScan()
    GoToState("active")

    ObjectReference[] linkedRefs = GetLinkedRefChain()
    int i = 0
    while i < linkedRefs.Length
        linkedRefs[i].Enable(FadeInObjects)
        i += 1
    endwhile

    if DisableWhileActive
        DisableNoWait()
    endif

    Utility.Wait(ActiveTimeSeconds)

    i = 0
    while i < linkedRefs.Length
        linkedRefs[i].Disable(FadeInObjects)
        i += 1
    endwhile

    if DisableWhileActive
        EnableNoWait()
    endif

    GoToState("waiting")
EndFunction

Event OnActivate(ObjectReference akActivator)
    RunScan()
EndEvent

State waiting
    Event OnActivate(ObjectReference akActivator)
        RunScan()
    EndEvent
EndState

State active
    Event OnActivate(ObjectReference akActivator)
    EndEvent
EndState
