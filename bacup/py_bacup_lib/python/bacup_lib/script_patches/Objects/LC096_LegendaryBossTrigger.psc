; TODO

Event OnInit()
    ActivationVolume = Self
    BossSpawnMarker = GetLinkedRef(LinkCustom01)
    CombatVolume = GetLinkedRef(LinkCustom02)
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If aBoss != None && !aBoss.IsDead()
        Return
    EndIf
    If BossSpawnMarker == None || ScorchedBoss == None
        Return
    EndIf
    aBoss = BossSpawnMarker.PlaceActorAtMe(ScorchedBoss, 3)
    If aBoss == None
        Return
    EndIf
    If FilterKeyword != None
        aBoss.AddKeyword(FilterKeyword)
    EndIf
    If Sandbox != None
        aBoss.AddKeyword(Sandbox)
    EndIf
    If Hold != None
        aBoss.AddKeyword(Hold)
    EndIf
    If HoldPreferred != None
        aBoss.AddKeyword(HoldPreferred)
    EndIf
    If HoldEngaged != None
        aBoss.AddKeyword(HoldEngaged)
    EndIf
    If CombatVolume != None
        aBoss.SetLinkedRef(CombatVolume)
    EndIf
    If Game.GetPlayer().GetValue(LC096_FirstEntry) == 0
        Game.GetPlayer().SetValue(LC096_FirstEntry, 1)
    EndIf
EndEvent
