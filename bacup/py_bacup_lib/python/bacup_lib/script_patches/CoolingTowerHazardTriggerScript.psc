; Replace FO76's GetLocalPlayer call and make the actor-base sex lookup explicit
; for FO4. The source decompiler otherwise emits an invalid cast/member chain.

Event OnInit()
    ActorBase playerBase = Game.GetPlayer().GetBaseObject() as ActorBase
    isLocalPlayerMale = playerBase != None && playerBase.GetSex() == 0
EndEvent

Function ClientPlayCoolingTowerHazardSFX()
    Float currentTime = Utility.GetCurrentRealTime()
    If SFXTimestamp + CONST_SFXDelay < currentTime
        SFXTimestamp = currentTime
        If isLocalPlayerMale
            If VOCPlayerMaleAHit != None
                VOCPlayerMaleAHit.Play(Game.GetPlayer())
            EndIf
        ElseIf VOCPlayerFemaleAHit != None
            VOCPlayerFemaleAHit.Play(Game.GetPlayer())
        EndIf
    EndIf
EndFunction
