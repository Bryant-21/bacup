Event OnActivate(ObjectReference akActionRef)
    If GetState() == "coolingdown"
        If RelayTowerLootCacheQuest != None && RelayTowerLootCacheQuest.IsRunning()
            If RelayTowerCoolDownMessage != None
                RelayTowerCoolDownMessage.Show()
            EndIf
            Return
        EndIf
        GoToState("CooledDown")
    EndIf

    Int selectedAction = 2
    If RelayTowerButtonMessage != None
        selectedAction = RelayTowerButtonMessage.Show()
    EndIf

    If selectedAction == 0
        If RelayTowerLootCacheQuest != None && RelayTowerLootCacheQuest.IsRunning()
            If RelayTowerCoolDownMessage != None
                RelayTowerCoolDownMessage.Show()
            EndIf
        ElseIf RelayTowerLootCacheQuestStart != None
            If RelayTowerLootCacheQuestStart.SendStoryEventAndWait(None, Self, akActionRef)
                GoToState("coolingdown")
            EndIf
        EndIf
    ElseIf selectedAction == 1
        Bool installJammer = RelayTowerJammerConfirmationMessage == None
        If RelayTowerJammerConfirmationMessage != None
            installJammer = RelayTowerJammerConfirmationMessage.Show() == 0
        EndIf
        If installJammer
            ObjectReference[] linkedRefs = GetLinkedRefChain(LinkCustom01)
            If linkedRefs != None
                Int i = 0
                While i < linkedRefs.Length
                    If linkedRefs[i] != None
                        linkedRefs[i].Activate(akActionRef)
                    EndIf
                    i += 1
                EndWhile
            EndIf
        EndIf
    EndIf
EndEvent
