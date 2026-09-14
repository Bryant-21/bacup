Event OnAliasInit()
    ObjectReference primaryGenerator = Game.GetFormFromFile(0x003DA80F, "SeventySix.esm") as ObjectReference
    ObjectReference powerButton = Alias_MTR07_EarthPowerButton.GetReference()
    If GetReference() == primaryGenerator && powerButton != None
        RegisterForRemoteEvent(powerButton, "OnActivate")
    EndIf
EndEvent

Event OnAliasShutdown()
    ObjectReference powerButton = Alias_MTR07_EarthPowerButton.GetReference()
    If powerButton != None
        UnregisterForRemoteEvent(powerButton, "OnActivate")
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Quest earthQuest = GetOwningQuest()
    ObjectReference powerButton = Alias_MTR07_EarthPowerButton.GetReference()
    If akSender == powerButton && akActionRef == Game.GetPlayer() && earthQuest.IsRunning() && earthQuest.IsStageDone(70) && !earthQuest.IsStageDone(200)
        earthQuest.SetStage(200)
    EndIf
EndEvent
