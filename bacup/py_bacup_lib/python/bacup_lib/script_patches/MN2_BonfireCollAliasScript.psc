; The forest bonfires are MN2_BonfireScript objects: lighting one scores and turns its fire on.
Function ResetRef(ObjectReference akRef)
    Parent.ResetRef(akRef)
    MN2_BonfireScript bonfire = akRef as MN2_BonfireScript
    If bonfire != None
        bonfire.ToggleBonfire(False)
    EndIf
EndFunction

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !MischiefOpen() || !ClaimRef(akSenderRef)
        Return
    EndIf
    MN2_BonfireScript bonfire = akSenderRef as MN2_BonfireScript
    If bonfire != None
        bonfire.ToggleBonfire(True)
    EndIf
    ScoreMischief(akSenderRef, player, 1.0)
EndEvent
