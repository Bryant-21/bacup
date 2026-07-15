Event OnUnload()
    ObjectReference targetRef = GetRef()
    if targetRef != None
        targetRef.DisableNoWait()
    endif
EndEvent
