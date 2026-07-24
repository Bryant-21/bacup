Function Fragment_End(Actor akActor)
    If AubrieEnableParent
        ObjectReference enableParent = AubrieEnableParent.GetReference()
        If enableParent
            enableParent.Enable()
        EndIf
    EndIf
EndFunction
