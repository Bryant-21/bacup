Event OnDeath(Actor akKiller)
    if flameThrowerKeyword == None
        return
    endif

    ObjectReference[] linkedRefs = GetLinkedRefChain(flameThrowerKeyword, 100)
    int i = 0
    while i < linkedRefs.Length
        TrapBase linkedTrap = linkedRefs[i] as TrapBase
        if linkedTrap != None
            linkedTrap.GoToState("Disarm")
        endif
        i += 1
    endwhile
EndEvent
