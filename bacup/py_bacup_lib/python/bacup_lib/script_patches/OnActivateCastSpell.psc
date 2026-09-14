Event OnActivate(ObjectReference akActionRef)
    If PlayerOnly && akActionRef != Game.GetPlayer()
        Return
    EndIf
    If SpellToCast == None
        Return
    EndIf

    If SelfCast
        SpellToCast.Cast(akActionRef, akActionRef)
    Else
        SpellToCast.Cast(Self, akActionRef)
    EndIf
EndEvent
