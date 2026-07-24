Event OnLoad()
    Actor owner = InstanceOwner.GetActorReference()
    If owner == None
        Return
    EndIf

    If owner.GetValue(AllowCleanUpValue) >= CleanupValueAmount
        Actor target = Self.GetActorReference()
        If target != None
            target.Disable()
        EndIf
    ElseIf owner.GetValue(KillNPCValue) >= KillValueAmount
        Actor target = Self.GetActorReference()
        If target != None
            target.Kill()
        EndIf
    EndIf
EndEvent
