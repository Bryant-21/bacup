Event OnAliasInit()
    ResetInstalledParts()
EndEvent

Event OnAliasReset()
    ResetInstalledParts()
EndEvent

Function ResetInstalledParts()
    B21InstalledParts = 0
EndFunction

Int Function GetInstalledParts()
    Return B21InstalledParts
EndFunction

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || ItemToRemove == None
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    Storm_E01_Dangerous eventScript = owningQuest as Storm_E01_Dangerous
    ; The stock FO4 counter keeps its StopCounting state across runs, so only its bound target and stage are read.
    counter = owningQuest as DefaultCounterQuestA
    If owningQuest == None || eventScript == None || counter == None || !owningQuest.IsRunning()
        Return
    EndIf
    If !owningQuest.IsStageDone(eventScript.iHarvesterPartsCollectionStage) || owningQuest.IsStageDone(counter.MyStage)
        Return
    EndIf

    Int carried = playerRef.GetItemCount(ItemToRemove)
    If carried <= 0
        If NotEnoughItemMSG != None
            NotEnoughItemMSG.Show()
        EndIf
        Return
    EndIf

    Int needed = counter.TargetValue - B21InstalledParts
    Int toInstall = carried
    If toInstall > needed
        toInstall = needed
    EndIf
    If toInstall > 0
        playerRef.RemoveItem(ItemToRemove, toInstall, True)
        Int installed = carried - playerRef.GetItemCount(ItemToRemove)
        If installed > 0
            B21InstalledParts += installed
            Int shownCount = B21InstalledParts
            If shownCount > counter.TargetValue
                shownCount = counter.TargetValue
            EndIf
            eventScript.SetInstalledPartCount(shownCount as Float)
            If installed == 1
                If OneRemovedMSG != None
                    OneRemovedMSG.Show()
                EndIf
            ElseIf ItemsRemovedMSG != None
                ItemsRemovedMSG.Show(installed as Float)
            EndIf
        EndIf
    EndIf

    If B21InstalledParts >= counter.TargetValue && !owningQuest.IsStageDone(counter.MyStage)
        owningQuest.SetStage(counter.MyStage)
    EndIf
EndEvent
