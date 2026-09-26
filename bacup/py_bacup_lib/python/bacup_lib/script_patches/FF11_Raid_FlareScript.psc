Event OnActivate(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If akActionRef != Game.GetPlayer() || owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    If !owningQuest.IsStageDone(25) || owningQuest.IsStageDone(100)
        Return
    EndIf

    ObjectReference launcher = Origin
    If launcher == None
        launcher = GetReference()
    EndIf
    If FF11_FlareSpell != None && launcher != None
        FF11_FlareSpell.Cast(launcher, Target)
    EndIf
    If ExplosionElectricalSmall != None && launcher != None
        launcher.PlaceAtMe(ExplosionElectricalSmall, 1, False, False, True)
    EndIf

    owningQuest.SetStage(100)
EndEvent
