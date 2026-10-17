; MessageToShow offers one design per EnableLinkedRefKeywords entry; the chosen decal replaces the blank wall
; (DisableLinkedRefKeyword). A new run restores the blank wall.
Function ResetRef(ObjectReference akRef)
    Parent.ResetRef(akRef)
    If EnableLinkedRefKeywords != None
        Int index = 0
        While index < EnableLinkedRefKeywords.Length
            ObjectReference decal = akRef.GetLinkedRef(EnableLinkedRefKeywords[index])
            If decal != None
                decal.Disable()
            EndIf
            index += 1
        EndWhile
    EndIf
    If DisableLinkedRefKeyword != None && akRef.GetLinkedRef(DisableLinkedRefKeyword) != None
        akRef.GetLinkedRef(DisableLinkedRefKeyword).Enable()
    EndIf
EndFunction

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !MischiefOpen() || MessageToShow == None || EnableLinkedRefKeywords == None
        Return
    EndIf
    If KeywordToAddOnActivate != None && akSenderRef.HasKeyword(KeywordToAddOnActivate)
        Return
    EndIf
    Int choice = MessageToShow.Show()
    If choice < 0 || choice >= EnableLinkedRefKeywords.Length || !ClaimRef(akSenderRef)
        Return
    EndIf
    ObjectReference decal = akSenderRef.GetLinkedRef(EnableLinkedRefKeywords[choice])
    If decal != None
        decal.Enable()
    EndIf
    If DisableLinkedRefKeyword != None && akSenderRef.GetLinkedRef(DisableLinkedRefKeyword) != None
        akSenderRef.GetLinkedRef(DisableLinkedRefKeyword).Disable()
    EndIf
    ScoreMischief(akSenderRef, player, 1.0)
EndEvent
