Event OnAliasInit()
    Actor aubrieRef = GetActorReference()
    ObjectReference enableParent = AubrieEnableParent.GetReference()
    If aubrieRef && enableParent
        aubrieRef.SetLinkedRef(enableParent)
    EndIf
EndEvent

Function PrepareForCave()
    Actor aubrieRef = GetActorReference()
    ObjectReference enableParent = AubrieEnableParent.GetReference()
    If enableParent
        enableParent.Enable()
    EndIf
    If aubrieRef
        If enableParent
            aubrieRef.SetLinkedRef(enableParent)
        EndIf
        aubrieRef.Enable()
        aubrieRef.EvaluatePackage()
    EndIf
EndFunction

Function SendHome()
    Actor aubrieRef = GetActorReference()
    ObjectReference enableParent = AubrieEnableParent.GetReference()
    If aubrieRef
        If enableParent
            aubrieRef.SetLinkedRef(enableParent)
        EndIf
        aubrieRef.EvaluatePackage()
    EndIf
EndFunction
