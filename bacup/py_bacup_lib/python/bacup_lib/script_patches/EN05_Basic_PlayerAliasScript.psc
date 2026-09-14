Bool Function EN05Basic_IsTrackingOutfit()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning()
        Return False
    EndIf
    Return owner.GetStageDone(iTrackOutfitStage) && !owner.GetStageDone(iStopTrackingOutfitStage)
EndFunction

Function EN05Basic_UpdateOutfitObjective(Form akBaseObject, Bool abEquipped)
    If akBaseObject == None || !EN05Basic_IsTrackingOutfit()
        Return
    EndIf

    Quest owner = GetOwningQuest()

    If EN05_MilitaryFatiguesFormlist != None && EN05_MilitaryFatiguesFormlist.HasForm(akBaseObject)
        owner.SetObjectiveCompleted(10, abEquipped)
    EndIf
    If EN05_MilitaryHelmetFormlist != None && EN05_MilitaryHelmetFormlist.HasForm(akBaseObject)
        owner.SetObjectiveCompleted(11, abEquipped)
    EndIf
EndFunction

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    EN05Basic_UpdateOutfitObjective(akBaseObject, True)
EndEvent

Event OnItemUnequipped(Form akBaseObject, ObjectReference akReference)
    EN05Basic_UpdateOutfitObjective(akBaseObject, False)
EndEvent
