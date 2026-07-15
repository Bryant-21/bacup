Event OnActivate(ObjectReference akActionRef)
    If PlayerOnly && akActionRef != Game.GetPlayer()
        Return
    EndIf
    If SpellToCast == None
        Return
    EndIf

    If SelfCast
        SpellToCast.Cast(Self, Self)
    Else
        SpellToCast.Cast(Self, akActionRef)
    EndIf
EndEvent
