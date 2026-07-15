Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && W05_RE_FlaviaInspectSkeletonMessage != None
        W05_RE_FlaviaInspectSkeletonMessage.Show()
    EndIf
EndEvent
