Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != None && RobotFriendlyFaction != None
        akTarget.AddToFaction(RobotFriendlyFaction)
    EndIf
EndEvent
