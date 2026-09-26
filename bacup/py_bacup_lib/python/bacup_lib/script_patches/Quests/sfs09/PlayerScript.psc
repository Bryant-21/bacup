; Event players share the lab's player faction, which the friendly creatures treat as allies.
Event OnAliasInit()
    SetPlayerFactionMembership(True)
EndEvent

Event OnAliasShutdown()
    SetPlayerFactionMembership(False)
EndEvent

Function SetPlayerFactionMembership(Bool abMember)
    MasterQuest = SFS09_Habitat_Master
    If MasterScript == None
        MasterScript = SFS09_Habitat_Master as Quests:sfs09:arktosmasterscript
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MasterScript == None || MasterScript.SFS09_Habitat_PlayerFaction == None
        Return
    EndIf
    If abMember
        playerRef.AddToFaction(MasterScript.SFS09_Habitat_PlayerFaction)
    Else
        playerRef.RemoveFromFaction(MasterScript.SFS09_Habitat_PlayerFaction)
    EndIf
EndFunction
