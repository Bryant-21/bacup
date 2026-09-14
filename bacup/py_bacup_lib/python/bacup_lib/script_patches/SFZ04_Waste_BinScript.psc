Event OnActivate(ObjectReference akActionRef)
    Actor playerRef
    If SFZ04Player != None
        playerRef = SFZ04Player.GetActorReference()
    EndIf
    If playerRef == None || akActionRef != playerRef || SFZ04_Waste_Core == None
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsStageDone(200) || owningQuest.IsStageDone(1000)
        Return
    EndIf
    If playerRef.GetItemCount(SFZ04_Waste_Core) < 3
        Return
    EndIf

    playerRef.RemoveItem(SFZ04_Waste_Core, 3, True)
    owningQuest.SetStage(1000)
EndEvent
